use crate::{
    types::{DottedVersionVector, Event, EventError, EventPayload},
    validation::EventValidator,
};
use sha2::Digest;
use sled::{Db, Transactional, Tree};
use std::{path::Path, sync::Arc};
use tracing::{debug, error, info, instrument, warn, Span};

const METADATA_TREE: &[u8] = b"metadata";
const EVENTS_TREE: &[u8] = b"events";

const HEAD_KEY: &[u8] = b"head_offset";
const CAUSALITY_KEY: &[u8] = b"causality_dvv";
const PREV_HASH_KEY: &[u8] = b"prev_hash";

/// Pre-signed event payload with signature metadata.
///
/// The signature must be created by the caller over the canonical event bytes.
/// The caller is responsible for:
/// - Authenticating the user
/// - Generating proper canonical bytes
/// - Signing with the appropriate key
/// - Encoding the signature (e.g., base64)
#[derive(Debug, Clone)]
pub struct SignedPayload<P: EventPayload> {
    pub payload: P,
    pub signer_did: String,
    pub signature: String,
    pub signature_type: String,
}

/// A durable, indexed, append-only event store powered by Sled.
pub struct EventStore {
    db: Db,
    metadata: Tree,
    events: Tree,
    validator: Arc<EventValidator>,
    stream_id: String,
    space_id: String,
}

impl EventStore {
    /// Opens or creates a new event store at the given path.
    #[instrument(skip(path, validator), fields(stream_id, space_id))]
    pub fn new<P: AsRef<Path>>(
        path: P,
        stream_id: String,
        space_id: String,
        validator: EventValidator,
    ) -> Result<Self, EventError> {
        let path_str = path.as_ref().display().to_string();

        info!(
            path = %path_str, stream_id = %stream_id, space_id = %space_id,
            "Opening event store"
        );

        let db = sled::open(path).map_err(|e| {
            error!(path = %path_str, error = %e, "Failed to open Sled database");
            e
        })?;

        let metadata = db.open_tree(METADATA_TREE)?;
        let events = db.open_tree(EVENTS_TREE)?;

        let event_count = events.len();
        info!(
            path = %path_str,
            stream_id = %stream_id,
            space_id = %space_id,
            event_count = event_count,
            "Event store opened successfully"
        );

        Ok(Self {
            db,
            metadata,
            events,
            validator: Arc::new(validator),
            stream_id,
            space_id,
        })
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Appends a pre-signed event to the store.
    ///
    /// # Arguments
    /// * `signer_did` - The DID of the signer (must match signature)
    /// * `signed_payload` - The payload with its signature
    ///
    /// # Caller Responsibilities
    /// The caller MUST:
    /// 1. Authenticate the signer and verify they own the DID
    /// 2. Create proper canonical event bytes
    /// 3. Sign those bytes with the signer's private key
    /// 4. Provide signature in the correct format (base64, hex, etc.)
    /// 5. Specify the correct signature_type
    ///
    /// # Event Store Responsibilities
    /// This method will:
    /// 1. Validate payload against JSON schema
    /// 2. Validate signature format (non-empty, type specified)
    /// 3. Generate event ID (ULID) and timestamp
    /// 4. Retrieve current state (prev_hash, causality, offset)
    /// 5. Create event with provided signature
    /// 6. Compute hash of canonical bytes (excluding signature)
    /// 7. Update hash chain, causality, and offset atomically
    /// 8. Persist event with full durability (flush)
    ///
    /// # Hash Chain vs Signature
    /// - **Hash Chain**: Prevents reordering/deletion of events
    /// - **Signature**: Proves who created the event
    /// These are independent mechanisms serving different purposes.
    ///
    /// # No Signature Verification
    /// This method does NOT verify signatures cryptographically because:
    /// - Verification requires DID resolution (network calls)
    /// - Different signature types need different verification logic
    /// - Verification is expensive (O(n) on reads)
    ///
    /// Signature verification should happen at:
    /// - API boundaries (before presenting to clients)
    /// - Downstream handlers (if they need trust guarantees)
    /// - Replication endpoints (when receiving from peers)
    ///
    /// # Example
    /// ```ignore
    /// // After authentication in API layer
    /// let auth_ctx = authenticate_user().await?;
    /// let signature = auth_ctx.sign(&payload_bytes)?;
    ///
    /// let signed_payload = SignedPayload {
    ///     payload: MyPayload { field: "value" },
    ///     signature: base64::encode(&signature),
    ///     signature_type: "Ed25519",
    /// };
    ///
    /// let event = store.append(signed_payload)?;
    /// ```
    #[instrument(
        skip(self, signed_payload),
        fields(
            event_type = P::TYPE,
            schema_version = P::VERSION,
            signer_did,
            sig_type = %signed_payload.signature_type,
            event_id,
            offset
        )
    )]
    pub fn append<P: EventPayload>(
        &self,
        signed_payload: SignedPayload<P>,
    ) -> Result<Event, EventError> {
        let signer_did = signed_payload.signer_did.as_str();
        Span::current().record("signer_did", signer_did);

        // 1. Validate payload against schema
        debug!("Serializing event payload");
        let payload_value = serde_json::to_value(&signed_payload.payload)?;

        debug!("Validating event payload against schema");
        self.validator
            .validate(&payload_value, P::TYPE, P::VERSION)?;

        // 2. Validate signature format (NOT cryptographic verification)
        if signed_payload.signature.is_empty() {
            warn!(
                signer_did = signer_did,
                "Attempted to append event with empty signature"
            );
            return Err(EventError::InvalidPayload);
        }
        if signed_payload.signature_type.is_empty() {
            warn!(
                signer_did = signer_did,
                "Attempted to append event with empty signature type"
            );
            return Err(EventError::InvalidPayload);
        }

        // 3. Generate event ID and timestamp
        let event_id = ulid::Ulid::new();
        let timestamp = chrono::Utc::now();

        Span::current().record("event_id", &event_id.to_string());
        debug!(
            event_id = %event_id,
            timestamp = %timestamp,
            "Generated event ID and timestamp"
        );

        // 4. Perform atomic append within transaction
        let result = (&self.metadata, &self.events)
            .transaction(|(metadata, events)| {
                // Get current head offset
                let head_opt = metadata
                    .get(HEAD_KEY)?
                    .map(|bytes| {
                        let arr: [u8; 8] = bytes.as_ref().try_into().map_err(|_| {
                            sled::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid offset bytes",
                            ))
                        })?;
                        Ok::<u64, sled::Error>(u64::from_be_bytes(arr))
                    })
                    .transpose()?;

                // Get current causality vector
                let causality: DottedVersionVector = metadata
                    .get(CAUSALITY_KEY)?
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .unwrap_or_default();

                // Get previous hash
                let prev_hash = metadata
                    .get(PREV_HASH_KEY)?
                    .map(|bytes| {
                        String::from_utf8(bytes.to_vec()).map_err(|_| {
                            sled::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid hash UTF-8",
                            ))
                        })
                    })
                    .transpose()?
                    .unwrap_or_else(|| "0".repeat(64)); // Genesis event

                // Calculate new offset
                let new_offset = match head_opt {
                    None => 0,
                    Some(h) => h + 1,
                };

                // Increment causality for this actor
                let mut new_causality = causality;
                new_causality.increment(signer_did);

                // Create event with provided signature
                let event = Event {
                    id: event_id,
                    ts: timestamp,
                    event_type: P::TYPE.to_string(),
                    schema_version: P::VERSION,
                    payload: payload_value.clone(),
                    causality: new_causality.clone(),
                    prev_hash,
                    signer: signer_did.to_string(),
                    sig: signed_payload.signature.clone(),
                    sig_type: signed_payload.signature_type.clone(),
                    trust_refs: vec![],
                    redactable: false,
                };

                // Compute hash of canonical event (EXCLUDES signature)
                // This is critical: hash is computed BEFORE signature is part of the event
                let canonical_bytes = event.canonical_bytes().map_err(|e| {
                    sled::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to create canonical bytes: {}", e),
                    ))
                })?;
                let new_hash = format!("{:x}", sha2::Sha256::digest(&canonical_bytes));

                debug!(
                    new_hash = %new_hash,
                    prev_hash = %event.prev_hash,
                    "Computed event hash for chain"
                );

                // Serialize complete event for storage (includes signature)
                let event_bytes = serde_json::to_vec(&event).map_err(|e| {
                    sled::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to serialize event: {}", e),
                    ))
                })?;

                // Atomic write: update all metadata and persist event
                metadata.insert(HEAD_KEY, &new_offset.to_be_bytes())?;
                metadata.insert(
                    CAUSALITY_KEY,
                    serde_json::to_vec(&new_causality)
                        .map_err(|e| {
                            sled::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("Failed to serialize causality: {}", e),
                            ))
                        })?
                        .as_slice(),
                )?;
                metadata.insert(PREV_HASH_KEY, new_hash.as_bytes())?;
                events.insert(&new_offset.to_be_bytes(), event_bytes)?;

                Span::current().record("offset", new_offset);

                Ok((event, new_offset))
            })
            .map_err(|e: sled::transaction::TransactionError| {
                error!(
                    event_id = %event_id, error = ?e,
                    "Transaction failed during event append"
                );
                EventError::TransactionError(format!("{:?}", e))
            })?;

        // Ensure durability by flushing to disk
        self.db.flush()?;

        info!(
            event_id = %result.0.id,
            offset = result.1,
            event_type = %result.0.event_type,
            signer = %result.0.signer,
            sig_type = %result.0.sig_type,
            "Event appended successfully"
        );

        Ok(result.0)
    }

    /// Fetches a single event by its offset.
    #[instrument(skip(self), fields(offset))]
    pub fn get_event(&self, offset: u64) -> Result<Option<Event>, EventError> {
        debug!(offset = offset, "Fetching event");

        let bytes = self.events.get(offset.to_be_bytes())?;
        let result = bytes
            .map(|b| serde_json::from_slice(&b).map_err(EventError::Json))
            .transpose()?;

        if result.is_some() {
            debug!(offset = offset, "Event found");
        } else {
            debug!(offset = offset, "Event not found");
        }

        Ok(result)
    }

    /// Gets the current head offset of the event log.
    #[instrument(skip(self))]
    pub fn get_head_offset(&self) -> Result<u64, EventError> {
        let offset = self
            .metadata
            .get(HEAD_KEY)?
            .map(|bytes| u64::from_be_bytes(bytes.as_ref().try_into().unwrap()))
            .ok_or(EventError::LogIsEmpty)?;
        debug!(head_offset = offset, "Retrieved head offset");
        Ok(offset)
    }

    /// Gets the current causality state.
    #[instrument(skip(self))]
    pub fn get_causality(&self) -> Result<DottedVersionVector, EventError> {
        let causality = self
            .metadata
            .get(CAUSALITY_KEY)?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(EventError::Json))
            .transpose()?
            .unwrap_or_else(|| DottedVersionVector::default());

        debug!(
            num_actors = causality.clocks.len(),
            "Retrieved causality state"
        );

        Ok(causality)
    }

    /// Gets the previous hash.
    #[instrument(skip(self))]
    pub fn get_prev_hash(&self) -> Result<String, EventError> {
        let hash = self
            .metadata
            .get(PREV_HASH_KEY)?
            .map(|bytes| String::from_utf8(bytes.to_vec()).unwrap())
            .unwrap_or_else(|| "0".repeat(64));

        debug!(prev_hash = %hash, "Retrieved previous hash");
        Ok(hash)
    }

    /// Returns an iterator over all events in the store.
    pub fn iter_events(&self) -> impl Iterator<Item = Result<Event, EventError>> + '_ {
        self.events.iter().map(|result| {
            result
                .map_err(EventError::Database)
                .and_then(|(_, bytes)| serde_json::from_slice(&bytes).map_err(EventError::Json))
        })
    }

    /// Returns the number of events in the store.
    #[instrument(skip(self))]
    pub fn event_count(&self) -> Result<usize, EventError> {
        let count = self.events.len();
        debug!(count = count, "Retrieved event count");
        Ok(count)
    }

    /// Returns a forward-only iterator over events starting from a given offset.
    #[instrument(skip(self))]
    pub fn iter_from(&self, start_offset: u64) -> EventIterator {
        debug!(start_offset = start_offset, "Creating event iterator");
        EventIterator {
            tree: self.events.clone(),
            current_offset: start_offset,
        }
    }

    /// Returns an iterator over a range of events [start, end).
    #[instrument(skip(self))]
    pub fn iter_range(&self, start_offset: u64, end_offset: u64) -> EventRangeIterator {
        debug!(
            start_offset = start_offset,
            end_offset = end_offset,
            "Creating range iterator"
        );
        EventRangeIterator {
            tree: self.events.clone(),
            current_offset: start_offset,
            end_offset,
        }
    }
}

/// An iterator for traversing events in the Sled store.
pub struct EventIterator {
    tree: Tree,
    current_offset: u64,
}

impl Iterator for EventIterator {
    type Item = Result<(u64, Event), EventError>;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.current_offset.to_be_bytes();
        match self.tree.range(start..).next() {
            Some(Ok((offset_bytes, event_bytes))) => {
                let offset = u64::from_be_bytes(offset_bytes.as_ref().try_into().unwrap());
                self.current_offset = offset.saturating_add(1);

                let event_result =
                    serde_json::from_slice::<Event>(&event_bytes).map_err(EventError::Json);

                Some(event_result.map(|event| (offset, event)))
            }
            Some(Err(e)) => Some(Err(EventError::Database(e))),
            None => None,
        }
    }
}

/// An iterator for traversing a range of events in the Sled store.
pub struct EventRangeIterator {
    tree: Tree,
    current_offset: u64,
    end_offset: u64,
}

impl Iterator for EventRangeIterator {
    type Item = Result<(u64, Event), EventError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_offset >= self.end_offset {
            return None;
        }

        let start = self.current_offset.to_be_bytes();

        match self.tree.range(start..).next() {
            Some(Ok((offset_bytes, event_bytes))) => {
                let offset = u64::from_be_bytes(offset_bytes.as_ref().try_into().unwrap());

                if offset >= self.end_offset {
                    return None;
                }

                self.current_offset = offset.saturating_add(1);

                let event_result =
                    serde_json::from_slice::<Event>(&event_bytes).map_err(EventError::Json);

                Some(event_result.map(|event| (offset, event)))
            }
            Some(Err(e)) => Some(Err(EventError::Database(e))),
            None => None,
        }
    }
}
