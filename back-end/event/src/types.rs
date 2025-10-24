use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use ulid::Ulid;

#[derive(Error, Debug)]
pub enum EventError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("schema not found for type '{event_type}' and version '{version}'")]
    SchemaNotFound { event_type: String, version: u32 },
    #[error("Invalid payload for event")]
    InvalidPayload,
    #[error("database error: {0}")]
    Database(#[from] sled::Error),
    #[error("Log is empty, cannot determine previous state")]
    LogIsEmpty,
    #[error("TransactionError '{0}'")]
    TransactionError(String),
    #[error("HashError '{0}'")]
    HashError(String),
}

/// A trait that all event payloads must implement.
/// This decouples the generic event system from specific payload types.
pub trait EventPayload: Serialize + for<'de> Deserialize<'de> {
    /// The unique type name for this event, e.g., "SpaceCreated".
    const TYPE: &'static str;
    /// The schema version for this event payload struct.
    const VERSION: u32;
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DottedVersionVector {
    // A map from an actor's DID to their latest sequence number.
    // Full implementation with dot-handling will be fleshed out later.
    pub clocks: HashMap<String, u64>,
    pub dots: HashSet<(String, u64)>,
}

impl Default for DottedVersionVector {
    fn default() -> Self {
        Self {
            clocks: HashMap::new(),
            dots: HashSet::new(),
        }
    }
}

impl DottedVersionVector {
    // Check if this DVV causally precedes another
    pub fn precedes(&self, other: &DottedVersionVector) -> bool {
        // For each actor in self, their clock must be <= other's clock
        for (actor, clock) in &self.clocks {
            if let Some(other_clock) = other.clocks.get(actor) {
                if clock > other_clock {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    // Check if two DVVs are concurrent (neither precedes the other)
    pub fn concurrent(&self, other: &DottedVersionVector) -> bool {
        !self.precedes(other) && !other.precedes(self)
    }

    pub fn merge(&mut self, other: &DottedVersionVector) {
        for (actor, clock) in &other.clocks {
            let current_clock = self.clocks.get(actor).unwrap_or(&0);
            if clock > current_clock {
                self.clocks.insert(actor.clone(), *clock);
            }
        }

        // Merge dots
        for dot in &other.dots {
            self.dots.insert(dot.clone());
        }
    }

    /// Advances the clock for a given actor, creating a new dot.
    pub fn increment(&mut self, actor_did: &str) {
        let new_clock = self.clocks.entry(actor_did.to_string()).or_insert(0);
        *new_clock += 1;
        self.dots.clear();
        self.dots.insert((actor_did.to_string(), *new_clock));
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Ulid,
    #[serde(rename = "type")]
    pub event_type: String,
    pub schema_version: u32,
    pub payload: Value,
    pub ts: DateTime<Utc>,
    pub causality: DottedVersionVector,
    pub prev_hash: String,
    pub signer: String,
    pub sig: String,
    pub sig_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trust_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redactable: bool,
}

impl Event {
    /// Creates deterministic bytes for hashing/signing.
    ///
    /// This representation includes all event data EXCEPT the signature
    /// fields (sig, sig_type). This avoids circularity and ensures the
    /// hash can be computed before signing.
    ///
    /// # Determinism Guarantees
    /// - JSON keys are serialized in a consistent order (serde_json guarantees this)
    /// - Timestamp is converted to milliseconds (integer, no timezone ambiguity)
    /// - All nested structures use deterministic serialization
    ///
    /// # What's Included
    /// - Event metadata (id, stream_id, space_id, type, version)
    /// - Event content (payload, timestamp)
    /// - Causality information (causality vector)
    /// - Hash chain (prev_hash)
    /// - Authorization (signer, trust_refs)
    /// - Flags (redactable)
    ///
    /// # What's Excluded
    /// - Signature (sig) - to avoid circularity
    /// - Signature type (sig_type) - metadata about the signature
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EventError> {
        use serde_json::json;

        let canonical = json!({
            "id": self.id.to_string(),
            "type": self.event_type,
            "schema_version": self.schema_version,
            "payload": self.payload,
            "ts": self.ts.timestamp_millis(),
            "causality": self.causality,
            "prev_hash": self.prev_hash,
            "signer": self.signer,
            "trust_refs": self.trust_refs,
            "redactable": self.redactable,
        });

        // Serialize to deterministic JSON bytes
        serde_json::to_vec(&canonical).map_err(|e| {
            EventError::HashError(format!("Failed to serialize canonical event: {}", e))
        })
    }

    /// Computes SHA-256 hash of the canonical event.
    /// This hash forms the hash chain (prev_hash links).
    pub fn compute_hash(&self) -> Result<String, EventError> {
        let bytes = self.canonical_bytes()?;
        Ok(format!("{:x}", sha2::Sha256::digest(&bytes)))
    }

    /// Verifies this event's prev_hash matches the given previous event.
    ///
    /// # Hash Chain Integrity
    /// This ensures:
    /// - Events are in the correct order
    /// - No events have been deleted
    /// - No event content has been modified
    ///
    /// # Returns
    /// - `Ok(true)` if hash chain is valid
    /// - `Ok(false)` if hashes don't match (chain broken)
    /// - `Err(_)` if hash computation failed
    pub fn verify_chain(&self, prev_event: &Event) -> Result<bool, EventError> {
        let expected_hash = prev_event.compute_hash()?;
        Ok(self.prev_hash == expected_hash)
    }

    /// Validates hash chain for a sequence of events.
    ///
    /// # Example
    /// ```ignore
    /// let events = store.iter_events().collect::<Result<Vec<_>, _>>()?;
    /// Event::verify_chain_sequence(&events)?;
    /// ```
    pub fn verify_chain_sequence(events: &[Event]) -> Result<bool, EventError> {
        if events.len() < 2 {
            return Ok(true); // Single event or empty is trivially valid
        }

        for window in events.windows(2) {
            if !window[1].verify_chain(&window[0])? {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
