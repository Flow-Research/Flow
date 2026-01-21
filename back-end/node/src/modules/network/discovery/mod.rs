//! Content discovery module for the Flow network.
//!
//! This module handles:
//! - Content announcements via GossipSub
//! - Discovery of remote content
//! - Validation of incoming announcements
//! - Remote content indexing

mod announcement;
mod handler;
mod validation;

pub use announcement::{
    ContentAnnouncement, CONTENT_ANNOUNCE_TOPIC, MAX_CONTENT_SIZE, MAX_TIMESTAMP_DRIFT_SECS,
};
pub use handler::{DiscoveryError, DiscoveryHandler, DiscoveryResult, DiscoveryStats};
pub use validation::{ValidationError, ValidationPolicy};
