use libp2p::{
    request_response::ResponseChannel,
    Multiaddr, PeerId
};
use tokio::sync::oneshot;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Serialize, Deserialize, Debug)]
pub enum ConsensusMessage {
    Proposal { id: String, content: String },
    Vote { proposal_id: String, vote: bool },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VoteRequest {
    pub proposal: String,
    pub proposal_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VoteResponse {
    pub proposal_id: String,
    pub vote: bool,
}

#[derive(Debug)]
pub enum NetworkCommand {
    StartListening {
        addr: Multiaddr,
        bootstrap_nodes: Vec<Multiaddr>,
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