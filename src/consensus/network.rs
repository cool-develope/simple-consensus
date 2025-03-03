use crate::consensus::message::{ConsensusMessage, VoteRequest, VoteResponse};
use futures::StreamExt;
use libp2p::{
    core::{muxing::StreamMuxerBox, transport::Boxed, upgrade},
    gossipsub, identify, identity,
    kad::{self, QueryId},
    noise,
    request_response::{self, OutboundRequestId, ProtocolSupport, ResponseChannel},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, SwarmBuilder, Transport,
};
use std::{collections::HashMap, collections::HashSet, error::Error, time::Duration};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tracing::{debug, error, info, warn}; // Import tracing macros

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
    GetValue {
        key: String,
        response_channel: oneshot::Sender<Result<Option<Vec<u8>>, Box<dyn Error + Send>>>,
    },
    PutValue {
        key: String,
        value: Vec<u8>,
    },
}

pub struct NetworkManager {
    command_sender: mpsc::Sender<NetworkCommand>,
    event_receiver: mpsc::Receiver<NetworkEvent>,
}

impl NetworkManager {
    pub async fn new(local_key: identity::Keypair) -> Result<Self, Box<dyn Error>> {
        let (command_sender, command_receiver) = mpsc::channel(10);
        let (event_sender, event_receiver) = mpsc::channel(10);

        let swarm = SwarmBuilder::with_existing_identity(local_key.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| Self::create_behaviour(key).unwrap())?
            .build();

        let event_sender_clone = event_sender.clone(); // Clone for moving into EventLoop
        tokio::spawn(async move {
            EventLoop {
                swarm,
                command_receiver,
                event_sender: event_sender_clone,
                pending_dial: HashMap::new(),
                pending_get_value: HashMap::new(),
            }
            .run()
            .await;
        });

        Ok(Self {
            command_sender,
            event_receiver,
        })
    }

    fn create_behaviour(
        local_key: &identity::Keypair,
    ) -> Result<ConsensusNetworkBehaviour, Box<dyn Error>> {
        let local_peer_id = PeerId::from(local_key.public());

        let identify = identify::Behaviour::new(identify::Config::new(
            "/consensus/1.0.0".into(),
            local_key.public(),
        ));

        let kademlia =
            kad::Behaviour::new(local_peer_id, kad::store::MemoryStore::new(local_peer_id));

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
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

    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        self.event_receiver.recv().await
    }

    pub async fn send_command(
        &self,
        command: NetworkCommand,
    ) -> Result<(), mpsc::error::SendError<NetworkCommand>> {
        self.command_sender.send(command).await
    }
}

/// Handles the main event loop for the network.
struct EventLoop {
    swarm: Swarm<ConsensusNetworkBehaviour>,
    command_receiver: mpsc::Receiver<NetworkCommand>, // Changed to Receiver
    event_sender: mpsc::Sender<NetworkEvent>,         // Changed to Sender
    pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
    pending_get_value:
        HashMap<QueryId, oneshot::Sender<Result<Option<Vec<u8>>, Box<dyn Error + Send>>>>,
}

impl EventLoop {
    async fn run(mut self) {
        info!("EventLoop started");
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
                        info!("Command channel closed, exiting EventLoop");
                        return;
                    }
                },
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<NetworkBehaviourEvent>) {
        debug!("Swarm event: {:?}", event);
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                let network_event = NetworkEvent::NewListenAddr(address.clone());
                if let Err(e) = self.event_sender.send(network_event).await {
                    error!("Failed to send NewListenAddr event: {:?}", e);
                }
                info!("Listening on {:?}", address);
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("Connected to peer {:?}", peer_id);
                if let Err(e) = self
                    .event_sender
                    .send(NetworkEvent::PeerConnected(peer_id))
                    .await
                {
                    error!("Failed to send PeerConnected event: {:?}", e);
                }
                if let Some(response_sender) = self.pending_dial.remove(&peer_id) {
                    if let Err(e) = response_sender.send(Ok(())) {
                        warn!("Failed to send DialPeer response: {:?}", e);
                    }
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                info!("Disconnected from peer {:?}", peer_id);
                if let Err(e) = self
                    .event_sender
                    .send(NetworkEvent::PeerDisconnected(peer_id))
                    .await
                {
                    error!("Failed to send PeerDisconnected event: {:?}", e);
                }
                if let Some(response_sender) = self.pending_dial.remove(&peer_id) {
                    if let Err(e) = response_sender.send(Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Connection closed unexpectedly",
                    )))) {
                        warn!(
                            "Failed to send DialPeer error response (connection closed): {:?}",
                            e
                        );
                    }
                }
            }
            SwarmEvent::Behaviour(NetworkBehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
                connection_id: _,
            })) => {
                info!("Identify event received from {:?}: {:?}", peer_id, info);
            }
            SwarmEvent::Behaviour(NetworkBehaviourEvent::Gossipsub(
                gossipsub::Event::Message {
                    message_id,
                    propagation_source,
                    message,
                },
            )) => {
                let network_event = NetworkEvent::GossipsubMessage {
                    source: propagation_source,
                    message_id,
                    message: message.data.clone(),
                };
                if let Err(e) = self.event_sender.send(network_event).await {
                    error!("Failed to send GossipsubMessage event: {:?}", e);
                }
                debug!(
                    "Gossipsub message received from {:?}: {:?}",
                    propagation_source, message
                );
            }
            SwarmEvent::Behaviour(NetworkBehaviourEvent::Kademlia(
                kad::Event::OutboundQueryProgressed { id, result, .. },
            )) => match result {
                kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(record))) => {
                    if let Some(response_sender) = self.pending_get_value.remove(&id) {
                        let value = record.record.value;
                        if let Err(e) = response_sender.send(Ok(Some(value))) {
                            warn!("Failed to send GetValue response: {:?}", e);
                        }
                    }
                }
                kad::QueryResult::GetRecord(Err(kad::GetRecordError::NotFound { .. })) => {
                    if let Some(response_sender) = self.pending_get_value.remove(&id) {
                        if let Err(e) = response_sender.send(Ok(None)) {
                            warn!("Failed to send GetValue not found response: {:?}", e);
                        }
                    }
                    debug!("No record found for query {:?}", id);
                }
                kad::QueryResult::GetRecord(Err(err)) => {
                    if let Some(response_sender) = self.pending_get_value.remove(&id) {
                        let error_message = format!("Kademlia GetRecord failed: {:?}", err);
                        if let Err(e) = response_sender.send(Err(Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            error_message,
                        )))) {
                            warn!("Failed to send GetValue error response: {:?}", e);
                        }
                    }
                    error!("Kademlia GetRecord failed: {:?}", err);
                }
                kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                    debug!("Successfully put record with key: {:?}", key);
                }
                kad::QueryResult::PutRecord(Err(err)) => {
                    error!("Failed to put record: {:?}", err);
                }
                _ => {
                    debug!("Kademlia query {:?} progressed: {:?}", id, result);
                }
            },
            SwarmEvent::Behaviour(NetworkBehaviourEvent::RequestResponse(
                request_response::Event::Message {
                    peer,
                    message:
                        request_response::Message::Request {
                            request, channel, ..
                        },
                    connection_id: _,
                },
            )) => {
                let network_event = NetworkEvent::VoteRequest {
                    peer_id: peer,
                    request,
                    channel,
                };
                if let Err(e) = self.event_sender.send(network_event).await {
                    error!("Failed to send VoteRequest event: {:?}", e);
                }
                debug!("RequestResponse VoteRequest received from {:?}", peer);
            }
            SwarmEvent::Behaviour(NetworkBehaviourEvent::RequestResponse(
                request_response::Event::OutboundFailure {
                    peer,
                    request_id,
                    error,
                    connection_id,
                },
            )) => {
                error!(
                    "RequestResponse OutboundFailure to peer {:?} for request {:?}: {:?}",
                    peer, request_id, error
                );
            }
            SwarmEvent::Behaviour(NetworkBehaviourEvent::RequestResponse(
                request_response::Event::InboundFailure {
                    peer,
                    request_id,
                    error,
                    connection_id,
                },
            )) => {
                error!(
                    "RequestResponse InboundFailure from peer {:?} for request {:?}: {:?}",
                    peer, request_id, error
                );
            }
            _event => {
                debug!("Unhandled swarm event: {:?}", _event);
            }
        }
    }

    async fn handle_command(&mut self, command: NetworkCommand) {
        info!("Handling command: {:?}", command);
        match command {
            NetworkCommand::StartListening { addr, response } => {
                // Try to start listening on the provided address.
                let result = self.swarm.listen_on(addr.clone());
                match result {
                    Ok(_listener_id) => {
                        // ListenerId is not used in this example, but could be used to manage listeners.
                        info!("Listening on {:?}", addr);
                        // Send back a success response.
                        if let Err(e) = response.send(Ok(())) {
                            error!("Failed to send StartListening response: {:?}", e);
                        }
                    }
                    Err(e) => {
                        error!("Error listening on {}: {:?}", addr, e);
                        if let Err(send_err) = response.send(Err(Box::new(e))) {
                            error!(
                                "Failed to send error response for StartListening: {:?}",
                                send_err
                            );
                        }
                    }
                }
            }
            NetworkCommand::DialPeer {
                peer_id,
                addr,
                response,
            } => {
                // Optimistically add address to kademlia routing table.
                self.swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, addr.clone());
                debug!("Dialing peer {:?} at {:?}", peer_id, addr);
                match self.swarm.dial(
                    addr.clone()
                        .with(libp2p::multiaddr::Protocol::P2p(peer_id.clone())),
                ) {
                    Ok(_connection_id) => {
                        // ConnectionId is not used in this example, but could be used to manage connections.
                        self.pending_dial.insert(peer_id.clone(), response);
                        // ConnectionEstablishd event will handle the response sender upon successful connection.
                    }
                    Err(e) => {
                        error!(
                            "Error dialing peer {:?} at {}: {:?}",
                            peer_id,
                            addr.clone(),
                            e
                        );
                        if let Err(send_err) = response.send(Err(Box::new(e))) {
                            error!("Failed to send error response for DialPeer: {:?}", send_err);
                        }
                    }
                }
            }
            NetworkCommand::GetLocalPeerId { response } => {
                let local_peer_id = *self.swarm.local_peer_id();
                if let Err(e) = response.send(Ok(local_peer_id)) {
                    error!("Failed to send GetLocalPeerId response: {:?}", e);
                }
            }
            NetworkCommand::PutValue { key, value } => {
                let record = kad::Record {
                    key: kad::RecordKey::new(&key),
                    value,
                    publisher: None, // You might want to set a publisher for record updates/deletes
                    expires: None,   // Records are persistent by default in MemoryStore
                };
                match self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .put_record(record, kad::Quorum::One)
                {
                    Ok(query_id) => {
                        debug!("PutValue initiated with query id: {:?}", query_id);
                        // You can optionally track query_id for confirmation in Kademlia events if needed.
                    }
                    Err(e) => {
                        error!("Error putting record for key {}: {:?}", key, e);
                    }
                }
            }
            NetworkCommand::GetValue {
                key,
                response_channel,
            } => {
                let key = kad::RecordKey::new(&key);
                let query_id = self.swarm.behaviour_mut().kademlia.get_record(key);

                self.pending_get_value.insert(query_id, response_channel);
                debug!("GetValue initiated with query id: {:?}", query_id);
            }
            NetworkCommand::Broadcast(message) => {
                if let Err(e) = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gossipsub::IdentTopic::new("global-topic"), message)
                {
                    error!("Failed to broadcast message: {:?}", e);
                } else {
                    debug!("Broadcast message");
                }
            }
            NetworkCommand::SendVoteRequest {
                peer_id,
                request,
                response,
            } => {
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer_id, request);
                // TODO: Implement pending_vote_requests to track request_id and response channel
                warn!("SendVoteRequest not fully implemented, response channel will be ignored");
                if let Err(e) = response.send(Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "SendVoteRequest not fully implemented",
                )))) {
                    error!("Failed to send error response for SendVoteRequest: {:?}", e);
                }
            }
            NetworkCommand::RespondVote { response, channel } => {
                match self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response)
                {
                    Ok(_) => {
                        debug!("RespondVote sent");
                    }
                    Err(e) => {
                        error!("Failed to send vote response: {:?}", e);
                    }
                }
            }
            other => {
                warn!("Unhandled command: {:?}", other);
            }
        }
    }
}
