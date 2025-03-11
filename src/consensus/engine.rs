// engine.rs

use crate::consensus::message::{ConsensusMessage, NetworkCommand, VoteRequest, VoteResponse};
use crate::consensus::network::{NetworkEvent, NetworkManager};
use futures::FutureExt;
use libp2p::{Multiaddr, PeerId};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    sync::Arc,
    time::Duration,
};
use tokio::{
    select,
    sync::{mpsc, oneshot, Mutex},
};
use tracing::{debug, error, info, warn};

/// The simple state machine for our consensus engine.
#[derive(Debug, Clone)]
pub enum ConsensusState {
    /// No consensus activity is taking place.
    Idle,
    /// A proposal has been broadcast by the leader and we are gathering votes.
    Proposing {
        proposal: ConsensusMessage,
        /// Votes collected from peers.
        votes: HashMap<PeerId, bool>,
    },
    /// Consensus has been reached (or not) for the proposal.
    Finalized {
        proposal: ConsensusMessage,
        accepted: bool,
    },
}

/// The engine ties together the network manager and our consensus state.
pub struct Engine {
    /// Wrapped network manager for sending commands and receiving events.
    network_manager: Arc<Mutex<NetworkManager>>,
    /// Current consensus state.
    consensus_state: Arc<Mutex<ConsensusState>>,
    /// Set of connected peers.
    connected_peers: Arc<Mutex<HashSet<PeerId>>>,
    /// The local node’s PeerId.
    local_peer_id: PeerId,
    /// Current leader (the node with the smallest PeerId).
    leader: Arc<Mutex<PeerId>>,
}

pub struct EngineConfig {
    pub secret_key: [u8; 32],
    pub listen_address: String,
    pub bootstrap_nodes: Vec<String>,
}

impl Engine {
    /// Starts the node by creating the network manager, starting the listening service,
    /// and spawning background tasks for handling network events and leader election.
    pub async fn new(config: EngineConfig) -> Result<Self, Box<dyn Error>> {
        let listen_addr = config.listen_address.parse()?;
        let bootstrap_nodes = config
            .bootstrap_nodes
            .into_iter()
            .map(|addr| addr.parse())
            .collect::<Result<Vec<_>, _>>()?;
        // Create a network manager.
        let network_manager = NetworkManager::new(config.secret_key).await?;
        let network_manager = Arc::new(Mutex::new(network_manager));

        // Send a StartListening command.
        let (start_tx, start_rx) = oneshot::channel();
        {
            let nm = network_manager.lock().await;
            nm.send_command(NetworkCommand::StartListening {
                addr: listen_addr,
                bootstrap_nodes,
                response: start_tx,
            })
            .await?;
        }
        start_rx.await?.unwrap();
        info!("Listening started");

        // Retrieve the local peer ID.
        let (peer_tx, peer_rx) = oneshot::channel();
        {
            let nm = network_manager.lock().await;
            nm.send_command(NetworkCommand::GetLocalPeerId { response: peer_tx })
                .await?;
        }
        let local_peer_id = peer_rx.await?.unwrap();
        info!("Local PeerId: {:?}", local_peer_id);

        // Initialize the connected peers (starting with ourselves).
        let connected_peers = Arc::new(Mutex::new(HashSet::new()));
        {
            let mut peers = connected_peers.lock().await;
            peers.insert(local_peer_id);
        }

        // Initially, elect ourselves as leader.
        let leader = Arc::new(Mutex::new(local_peer_id));

        let consensus_state = Arc::new(Mutex::new(ConsensusState::Idle));

        let engine = Self {
            network_manager: network_manager.clone(),
            consensus_state: consensus_state.clone(),
            connected_peers: connected_peers.clone(),
            local_peer_id,
            leader: leader.clone(),
        };

        // Spawn background tasks.
        engine.spawn_event_loop();
        engine.spawn_leader_election_loop();

        Ok(engine)
    }

    /// Spawns the background event loop that processes network events.
    fn spawn_event_loop(&self) {
        let network_manager = self.network_manager.clone();
        let consensus_state = self.consensus_state.clone();
        let connected_peers = self.connected_peers.clone();
        let leader = self.leader.clone();
        let local_peer_id = self.local_peer_id;

        tokio::spawn(async move {
            loop {
                // Lock the network manager to receive the next event.
                let event_opt = {
                    let mut nm = network_manager.lock().await;
                    nm.next_event().await
                };
                if let Some(event) = event_opt {
                    match event {
                        NetworkEvent::PeerConnected(peer_id) => {
                            info!("Peer connected: {:?}", peer_id);
                            connected_peers.lock().await.insert(peer_id);
                        }
                        NetworkEvent::PeerDisconnected(peer_id) => {
                            info!("Peer disconnected: {:?}", peer_id);
                            connected_peers.lock().await.remove(&peer_id);
                        }
                        NetworkEvent::GossipsubMessage {
                            source,
                            message_id: _,
                            message,
                        } => {
                            // Try to decode the message as a ConsensusMessage.
                            if let Ok(consensus_msg) =
                                serde_json::from_slice::<ConsensusMessage>(&message)
                            {
                                debug!(
                                    "Received consensus message from {:?}: {:?}",
                                    source, consensus_msg
                                );
                                match consensus_msg {
                                    // A proposal has been broadcast by the leader.
                                    ConsensusMessage::Proposal { id, content } => {
                                        // If we are not the leader, automatically vote “yes”.
                                        let current_leader = *leader.lock().await;
                                        if local_peer_id != current_leader {
                                            info!("Non-leader node voting YES for proposal {}", id);
                                            // Create a VoteRequest.
                                            let vote_req = VoteRequest {
                                                proposal: content.clone(),
                                                proposal_id: id.clone(),
                                            };
                                            // For simplicity, we assume an automatic YES vote.
                                            let vote_msg = ConsensusMessage::Vote {
                                                proposal_id: id,
                                                vote: true,
                                            };
                                            // Broadcast the vote.
                                            if let Ok(vote_bytes) = serde_json::to_vec(&vote_msg) {
                                                let nm = network_manager.lock().await;
                                                if let Err(e) = nm
                                                    .send_command(NetworkCommand::Broadcast(
                                                        vote_bytes,
                                                    ))
                                                    .await
                                                {
                                                    error!("Failed to broadcast vote: {:?}", e);
                                                }
                                            }
                                        }
                                    }
                                    // A vote message coming back to the leader.
                                    ConsensusMessage::Vote { proposal_id, vote } => {
                                        // Only the leader should tally votes.
                                        let current_leader = *leader.lock().await;
                                        if local_peer_id == current_leader {
                                            let mut state = consensus_state.lock().await;
                                            if let ConsensusState::Proposing {
                                                ref proposal,
                                                ref mut votes,
                                            } = *state
                                            {
                                                // Check that the vote corresponds to the proposal we are proposing.
                                                if let ConsensusMessage::Proposal { id, .. } =
                                                    proposal
                                                {
                                                    if id == &proposal_id {
                                                        votes.insert(source, vote);
                                                        // Check for majority: simple majority over all connected peers.
                                                        let total =
                                                            connected_peers.lock().await.len();
                                                        let yes_votes =
                                                            votes.values().filter(|&&v| v).count();
                                                        if yes_votes > total / 2 {
                                                            info!(
                                                                "Consensus reached for proposal {}",
                                                                proposal_id
                                                            );
                                                            *state = ConsensusState::Finalized {
                                                                proposal: proposal.clone(),
                                                                accepted: true,
                                                            };
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                debug!("Received non-consensus message on gossipsub");
                            }
                        }
                        _ => {
                            debug!("Unhandled network event: {:?}", event);
                        }
                    }
                }
            }
        });
    }

    /// Spawns a periodic task to re-calculate the leader.
    /// The node with the smallest PeerId is elected as leader.
    fn spawn_leader_election_loop(&self) {
        let connected_peers = self.connected_peers.clone();
        let leader = self.leader.clone();
        tokio::spawn(async move {
            loop {
                {
                    let peers = connected_peers.lock().await;
                    if let Some(new_leader) = peers.iter().min() {
                        let mut current_leader = leader.lock().await;
                        if *new_leader != *current_leader {
                            *current_leader = *new_leader;
                            info!("New leader elected: {:?}", new_leader);
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    /// Proposes a consensus message. Only the leader may propose.
    /// This function updates the consensus state to Proposing, broadcasts the proposal,
    /// and then the event loop will collect votes.
    pub async fn propose(&self, proposal: ConsensusMessage) -> Result<(), Box<dyn Error>> {
        // Check that we are the leader.
        let current_leader = *self.leader.lock().await;
        if self.local_peer_id != current_leader {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Only the leader can propose",
            )));
        }
        // Update the consensus state.
        {
            let mut state = self.consensus_state.lock().await;
            *state = ConsensusState::Proposing {
                proposal: proposal.clone(),
                votes: HashMap::new(),
            };
        }
        // Broadcast the proposal over gossipsub.
        let msg_bytes = serde_json::to_vec(&proposal)?;
        {
            let nm = self.network_manager.lock().await;
            nm.send_command(NetworkCommand::Broadcast(msg_bytes))
                .await?;
        }
        info!("Proposal broadcasted by leader");
        Ok(())
    }

    /// Returns the current consensus state.
    pub async fn get_status(&self) -> ConsensusState {
        self.consensus_state.lock().await.clone()
    }

    /// Returns the set of connected peer IDs.
    pub async fn get_connected_peers(&self) -> HashSet<String> {
        self.connected_peers
            .lock()
            .await
            .iter()
            .map(|peer_id| peer_id.to_string())
            .collect()
    }
}
