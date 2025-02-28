use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum ConsensusMessage {
    Proposal { id: String, content: String },
    Vote { proposal_id: String, vote: bool },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VoteRequest {
    pub proposal_id: String,
    pub proposal: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VoteResponse {
    pub proposal_id: String,
    pub vote: bool,
}
