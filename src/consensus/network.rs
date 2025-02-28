use crate::consensus::message::{ConsensusMessage, VoteRequest, VoteResponse};
use futures::StreamExt;
use libp2p::{
    core::muxing::StreamMuxerBox,
    core::transport::Boxed,
    core::upgrade,
    gossipsub, identify, identity, kad, noise,
    request_response::{self, ProtocolSupport, ResponseChannel},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, SwarmBuilder, Transport,
};
use std::{collections::HashMap, collections::HashSet, error::Error, time::Duration};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NetworkBehaviourEvent")]
pub struct ConsensusNetworkBehaviour {
    identify: identify::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    gossipsub: gossipsub::Behaviour,
    request_response: request_response::cbor::Behaviour<VoteRequest, VoteResponse>,
}

#[derive(Debug)]
pub enum NetworkBehaviourEvent {
    Identify(identify::Event),
    Kademlia(kad::Event),
    Gossipsub(gossipsub::Event),
    RequestResponse(request_response::Event<VoteRequest, VoteResponse>),
}

impl From<identify::Event> for NetworkBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}
impl From<kad::Event> for NetworkBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        Self::Kademlia(event)
    }
}
impl From<gossipsub::Event> for NetworkBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossipsub(event)
    }
}
impl From<request_response::Event<VoteRequest, VoteResponse>> for NetworkBehaviourEvent {
    fn from(event: request_response::Event<VoteRequest, VoteResponse>) -> Self {
        Self::RequestResponse(event)
    }
}

#[derive(Debug)]
pub enum NetworkEvent {
    NewListenAddr(Multiaddr),
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    GossipsubMessage {
        source: PeerId,
        message_id: gossipsub::MessageId,
        message: Vec<u8>,
    },
    VoteRequest {
        peer_id: PeerId,
        request: VoteRequest,
        channel: ResponseChannel<VoteResponse>,
    },
}

#[derive(Debug)]
pub enum NetworkCommand {
    StartListening {
        addr: Multiaddr,
        response: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    DialPeer {
        peer_id: PeerId,
        addr: Multiaddr,
        response: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    GetLocalPeerId {
        response: oneshot::Sender<Result<PeerId, Box<dyn Error + Send>>>,
    },
    Broadcast(Vec<u8>),
    SendVoteRequest {
        peer_id: PeerId,
        request: VoteRequest,
        response: oneshot::Sender<Result<VoteResponse, Box<dyn Error + Send>>>,
    },
    RespondVote {
        response: VoteResponse,
        channel: ResponseChannel<VoteResponse>,
    },
}

pub struct NetworkManager {
    command_sender: mpsc::Sender<NetworkCommand>,
    event_receiver: mpsc::Receiver<NetworkEvent>,
}

impl NetworkManager {
    pub async fn new(local_key: identity::Keypair) -> Result<Self, Box<dyn Error>> {
        let (command_sender, mut command_receiver) = mpsc::channel(10);
        let (event_sender, event_receiver) = mpsc::channel(10);

        let local_peer_id = PeerId::from(local_key.public());
        let behaviour = Self::create_behaviour(local_key.clone())?;
        let mut swarm = SwarmBuilder::with_existing_identity(local_key.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| behaviour)?
            .build();

        tokio::spawn(async move {
            // let mut pending_requests = HashMap::new();
            loop {
                tokio::select! {
                    event = swarm.select_next_some() => {
                        // Handle swarm events similarly to original code
                        // Convert to NetworkEvent and send via event_sender
                    },
                    command = command_receiver.recv() => {
                        // Handle NetworkCommand similarly to original EventLoop
                    }
                }
            }
        });

        Ok(Self {
            command_sender,
            event_receiver,
        })
    }

    fn create_behaviour(
        local_key: identity::Keypair,
    ) -> Result<ConsensusNetworkBehaviour, Box<dyn Error>> {
        let local_peer_id = PeerId::from(local_key.public());

        let identify = identify::Behaviour::new(identify::Config::new(
            "/consensus/1.0.0".into(),
            local_key.public(),
        ));

        let kademlia =
            kad::Behaviour::new(local_peer_id, kad::store::MemoryStore::new(local_peer_id));

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Anonymous,
            gossipsub::Config::default(),
        )?;

        let request_response = request_response::cbor::Behaviour::new(
            [(StreamProtocol::new("/vote/1.0.0"), ProtocolSupport::Full)],
            request_response::Config::default(),
        );

        Ok(ConsensusNetworkBehaviour {
            identify,
            kademlia,
            gossipsub,
            request_response,
        })
    }

    pub fn command_sender(&self) -> mpsc::Sender<NetworkCommand> {
        self.command_sender.clone()
    }

    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        self.event_receiver.recv().await
    }
}

/// Handles the main event loop for the network.
struct EventLoop {
    swarm: Swarm<ConsensusNetworkBehaviour>,
    command_receiver: mpsc::UnboundedReceiver<NetworkCommand>,
    event_sender: mpsc::UnboundedSender<NetworkEvent>,
}

impl EventLoop {
    async fn run(mut self) {
        loop {
            select! {
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                },
                command = self.command_receiver.recv() => {
                    if let Some(command) = command {
                        self.handle_command(command).await;
                    } else {
                        // All command senders have been dropped, so exit the event loop.
                        return;
                    }
                },
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<NetworkBehaviourEvent>) {
        match event {
            _ => { /* Handle swarm events */ }
        }
    }

    async fn handle_command(&mut self, command: NetworkCommand) {
        match command {
            _ => { /* Handle network commands */ }
        }
    }
}
