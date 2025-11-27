use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct NetworkStatusResponse {
    pub running: bool,
    pub peer_id: String,
    pub connected_peers: usize,
    pub known_peers: usize,
    pub uptime_secs: Option<u64>,
    pub subscribed_topics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PeersResponse {
    pub count: usize,
    pub peers: Vec<PeerInfoResponse>,
}

#[derive(Debug, Serialize)]
pub struct PeerInfoResponse {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub connection_count: usize,
    pub direction: String,
    pub discovery_source: String,
}

#[derive(Debug, Deserialize)]
pub struct DialPeerRequest {
    pub address: String,
}
