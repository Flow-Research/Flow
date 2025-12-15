//! Internal command types for NetworkManager to NetworkEventLoop communication

use super::types::{BootstrapResult, ConnectionCapacityStatus, ProviderConfirmationCallback};
use crate::modules::network::content_transfer::ContentTransferError;
use crate::modules::network::gossipsub::Message;
use crate::modules::network::peer_registry::{NetworkStats, PeerInfo};
use crate::modules::storage::content::{Block, ContentId, DocumentManifest};
use libp2p::kad::QueryId;
use libp2p::{Multiaddr, PeerId};
use std::collections::HashSet;
use tokio::sync::oneshot;

/// Commands sent to the NetworkManager event loop
pub(crate) enum NetworkCommand {
    /// Gracefully shutdown the network
    Shutdown,

    /// Query the current number of connected peers
    GetPeerCount(oneshot::Sender<usize>),

    /// Get list of all connected peers
    GetConnectedPeers(oneshot::Sender<Vec<PeerInfo>>),

    /// Get info about a specific peer
    GetPeerInfo {
        peer_id: PeerId,
        response: oneshot::Sender<Option<PeerInfo>>,
    },

    /// Dial a peer at a specific address
    DialPeer {
        address: Multiaddr,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Disconnect from a peer
    DisconnectPeer {
        peer_id: PeerId,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Get network statistics
    GetNetworkStats(oneshot::Sender<NetworkStats>),

    /// Get connection capacity status
    GetConnectionCapacity(oneshot::Sender<ConnectionCapacityStatus>),

    /// Trigger bootstrap process manually
    TriggerBootstrap {
        response: oneshot::Sender<BootstrapResult>,
    },

    /// Get list of bootstrap peer IDs
    GetBootstrapPeerIds(oneshot::Sender<Vec<PeerId>>),

    /// Subscribe to a topic
    Subscribe {
        topic: String,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Unsubscribe from a topic
    Unsubscribe {
        topic: String,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Publish a message
    Publish {
        message: Message,
        response: oneshot::Sender<Result<String, String>>,
    },

    /// Get subscribed topics
    GetSubscriptions(oneshot::Sender<Vec<String>>),

    /// Get mesh peers for a topic
    GetMeshPeers {
        topic: String,
        response: oneshot::Sender<Vec<PeerId>>,
    },

    /// Request a block from a peer
    RequestBlock {
        peer_id: PeerId,
        cid: ContentId,
        response: oneshot::Sender<Result<Block, ContentTransferError>>,
    },

    /// Request a manifest from a peer  
    RequestManifest {
        peer_id: PeerId,
        cid: ContentId,
        response: oneshot::Sender<Result<DocumentManifest, ContentTransferError>>,
    },

    /// Start providing content (DHT provider announcement)
    StartProviding {
        cid: ContentId,
        response: oneshot::Sender<Result<QueryId, String>>,
    },

    /// Stop providing content
    StopProviding {
        cid: ContentId,
        response: oneshot::Sender<Result<(), String>>,
    },

    /// Start providing content with optional confirmation callback
    StartProvidingWithCallback {
        cid: ContentId,
        callback: Option<ProviderConfirmationCallback>,
        response: oneshot::Sender<Result<QueryId, String>>,
    },

    /// Query DHT for content providers
    GetProviders {
        cid: ContentId,
        response: oneshot::Sender<Result<HashSet<PeerId>, String>>,
    },

    /// Check local block store for a block
    GetLocalBlock {
        cid: ContentId,
        response: oneshot::Sender<Option<Block>>,
    },
}
