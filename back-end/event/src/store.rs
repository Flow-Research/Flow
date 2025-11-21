use crate::{
    types::{DottedVersionVector, Event, EventError, EventPayload},
    validation::EventValidator,
};
use rocksdb::{
    BlockBasedOptions, ColumnFamilyDescriptor, DBIteratorWithThreadMode, Direction, IteratorMode,
    Options, WriteBatch, DB,
};
use sha2::Digest;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, instrument, warn, Span};

// Column family names
const CF_METADATA: &str = "metadata";
const CF_EVENTS: &str = "events";
const CF_INDEX: &str = "index";
const CF_SUBSCRIPTION_STATE: &str = "subscription_state";

// Metadata keys
const HEAD_KEY: &[u8] = b"head_offset";
const CAUSALITY_KEY: &[u8] = b"causality_dvv";
const PREV_HASH_KEY: &[u8] = b"prev_hash";

/// Configuration for RocksDB event store
#[derive(Debug, Clone)]
pub struct EventStoreConfig {
    pub max_open_files: i32,
    pub enable_compression: bool,
    pub write_buffer_size: usize,
    pub max_write_buffer_number: i32,
    pub enable_statistics: bool,
}

impl Default for EventStoreConfig {
    fn default() -> Self {
        Self {
            max_open_files: 1000,
            enable_compression: true,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            enable_statistics: false,
        }
    }
}

impl EventStoreConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            max_open_files: std::env::var("EVENT_STORE_MAX_OPEN_FILES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),

            enable_compression: std::env::var("EVENT_STORE_ENABLE_COMPRESSION")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),

            write_buffer_size: std::env::var("EVENT_STORE_WRITE_BUFFER_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(64 * 1024 * 1024),

            max_write_buffer_number: std::env::var("EVENT_STORE_MAX_WRITE_BUFFERS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),

            enable_statistics: std::env::var("EVENT_STORE_ENABLE_STATS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(false),
        }
    }
}

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

/// A durable, indexed, append-only event store powered by RocksDB.
pub struct RocksDbEventStore {
    db: Arc<DB>,
    validator: Arc<EventValidator>,
    stream_id: String,
    space_id: String,
    append_lock: Mutex<()>,
}

impl RocksDbEventStore {
    /// Opens or creates a new event store at the given path.
    #[instrument(skip(path, validator, config), fields(stream_id, space_id))]
    pub fn new<P: AsRef<Path>>(
        path: P,
        stream_id: String,
        space_id: String,
        validator: EventValidator,
        config: EventStoreConfig,
    ) -> Result<Self, EventError> {
        let path_str = path.as_ref().display().to_string();

        info!(
            path = %path_str, stream_id = %stream_id, space_id = %space_id,
            "Opening RocksDB event store"
        );

        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                error!(path = %path_str, error = %e, "Failed to create parent directory");
                EventError::Storage(format!("Failed to create directory: {}", e))
            })?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(config.max_open_files);
        opts.set_write_buffer_size(config.write_buffer_size);
        opts.set_max_write_buffer_number(config.max_write_buffer_number);
        opts.set_keep_log_file_num(10);

        if config.enable_compression {
            opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        }

        if config.enable_statistics {
            opts.enable_statistics();
        }

        // Define column families
        let cf_metadata = ColumnFamilyDescriptor::new(CF_METADATA, Options::default());
        let mut cf_events_opts = Options::default();
        // Optimize for sequential writes (append-only workload)
        cf_events_opts.set_level_compaction_dynamic_level_bytes(true);
        let cf_events = ColumnFamilyDescriptor::new(CF_EVENTS, cf_events_opts);

        // Optimize CF_INDEX for point lookups (hash → offset)
        let mut cf_index_opts = Options::default();
        let mut block_opts = BlockBasedOptions::default();
        // 10 bits per key, block-based (not full) filter
        block_opts.set_bloom_filter(10.0, false);
        cf_index_opts.set_block_based_table_factory(&block_opts);
        let cf_index = ColumnFamilyDescriptor::new(CF_INDEX, cf_index_opts);

        let cf_subscription =
            ColumnFamilyDescriptor::new(CF_SUBSCRIPTION_STATE, Options::default());

        // Open database with column families
        let db = DB::open_cf_descriptors(
            &opts,
            path,
            vec![cf_metadata, cf_events, cf_index, cf_subscription],
        )
        .map_err(|e| {
            error!(path = %path_str, error = %e, "Failed to open RocksDB database");
            EventError::Storage(format!("Failed to open database: {}", e))
        })?;

        let db = Arc::new(db);

        // Get event count
        let event_count = {
            let events_cf = db
                .cf_handle(CF_EVENTS)
                .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_EVENTS.to_string()))?;

            db.iterator_cf(&events_cf, IteratorMode::Start).count()
        };

        info!(
            path = %path_str,
            stream_id = %stream_id,
            space_id = %space_id,
            event_count = event_count,
            "RocksDB event store opened successfully"
        );

        Ok(Self {
            db,
            validator: Arc::new(validator),
            stream_id,
            space_id,
            append_lock: Mutex::new(()),
        })
    }

    /// Get reference to the underlying RocksDB instance
    pub fn db(&self) -> &Arc<DB> {
        &self.db
    }

    /// Appends a pre-signed event to the store using RocksDB WriteBatch for atomicity.
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
        // Acquire lock for the entire read-modify-write cycle
        let _guard = self.append_lock.lock().unwrap();

        let signer_did = signed_payload.signer_did.as_str();
        Span::current().record("signer_did", signer_did);

        // 1. Validate payload against schema
        debug!("Serializing event payload");
        let payload_value = serde_json::to_value(&signed_payload.payload)?;

        debug!("Validating event payload against schema");
        self.validator
            .validate(&payload_value, P::TYPE, P::VERSION)?;

        // 2. Validate signature format
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

        // 4. Get column family handles
        let metadata_cf = self
            .db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_METADATA.to_string()))?;
        let events_cf = self
            .db
            .cf_handle(CF_EVENTS)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_EVENTS.to_string()))?;
        let index_cf = self
            .db
            .cf_handle(CF_INDEX)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_INDEX.to_string()))?;

        // 5. Read current state
        let head_opt = self
            .db
            .get_cf(&metadata_cf, HEAD_KEY)?
            .map(|bytes| {
                let arr: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
                    EventError::InvalidData("Invalid head offset bytes".to_string())
                })?;
                Ok::<u64, EventError>(u64::from_be_bytes(arr))
            })
            .transpose()?;

        let causality: DottedVersionVector = self
            .db
            .get_cf(&metadata_cf, CAUSALITY_KEY)?
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        let prev_hash = self
            .db
            .get_cf(&metadata_cf, PREV_HASH_KEY)?
            .map(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|_| EventError::InvalidData("Invalid prev_hash UTF-8".to_string()))
            })
            .transpose()?
            .unwrap_or_else(|| "0".repeat(64)); // Genesis event

        // 6. Calculate new offset
        let new_offset = match head_opt {
            None => 0,
            Some(h) => h + 1,
        };

        // 7. Increment causality for this actor
        let mut new_causality = causality;
        new_causality.increment(signer_did);

        // 8. Create event with provided signature
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

        // 9. Compute hash of canonical event (EXCLUDES signature)
        let canonical_bytes = event.canonical_bytes()?;
        let new_hash = format!("{:x}", sha2::Sha256::digest(&canonical_bytes));

        debug!(
            new_hash = %new_hash,
            prev_hash = %event.prev_hash,
            "Computed event hash for chain"
        );

        // 10. Serialize complete event for storage (includes signature)
        let event_bytes = serde_json::to_vec(&event)?;

        // 11. Atomic write using WriteBatch
        let mut batch = WriteBatch::default();

        // Update metadata
        batch.put_cf(&metadata_cf, HEAD_KEY, new_offset.to_be_bytes());
        batch.put_cf(
            &metadata_cf,
            CAUSALITY_KEY,
            serde_json::to_vec(&new_causality)?,
        );
        batch.put_cf(&metadata_cf, PREV_HASH_KEY, new_hash.as_bytes());

        // Store event
        batch.put_cf(&events_cf, new_offset.to_be_bytes(), &event_bytes);

        // Store hash → offset index for deduplication
        batch.put_cf(&index_cf, new_hash.as_bytes(), new_offset.to_be_bytes());

        // Write batch atomically
        self.db.write(batch).map_err(|e| {
            error!(
                event_id = %event_id, error = %e,
                "WriteBatch failed during event append"
            );
            EventError::TransactionError(format!("WriteBatch failed: {}", e))
        })?;

        // 12. Ensure durability by flushing to disk
        self.db.flush_cf(&metadata_cf)?;
        self.db.flush_cf(&events_cf)?;
        self.db.flush_cf(&index_cf)?;

        Span::current().record("offset", new_offset);

        info!(
            event_id = %event.id,
            offset = new_offset,
            event_type = %event.event_type,
            signer = %event.signer,
            sig_type = %event.sig_type,
            hash = %new_hash,
            "Event appended successfully"
        );

        Ok(event)
    }

    /// Fetches a single event by its offset.
    #[instrument(skip(self), fields(offset))]
    pub fn get_event(&self, offset: u64) -> Result<Option<Event>, EventError> {
        debug!(offset = offset, "Fetching event");

        let events_cf = self
            .db
            .cf_handle(CF_EVENTS)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_EVENTS.to_string()))?;

        let bytes = self.db.get_cf(&events_cf, offset.to_be_bytes())?;
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
        let metadata_cf = self
            .db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_METADATA.to_string()))?;

        let offset = self
            .db
            .get_cf(&metadata_cf, HEAD_KEY)?
            .map(|bytes| {
                let arr: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
                    EventError::InvalidData("Invalid head offset bytes".to_string())
                })?;
                Ok::<u64, EventError>(u64::from_be_bytes(arr))
            })
            .transpose()?
            .ok_or(EventError::LogIsEmpty)?;

        debug!(head_offset = offset, "Retrieved head offset");
        Ok(offset)
    }

    /// Gets the current causality state.
    #[instrument(skip(self))]
    pub fn get_causality(&self) -> Result<DottedVersionVector, EventError> {
        let metadata_cf = self
            .db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_METADATA.to_string()))?;

        let causality = self
            .db
            .get_cf(&metadata_cf, CAUSALITY_KEY)?
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
        let metadata_cf = self
            .db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_METADATA.to_string()))?;

        let hash = self
            .db
            .get_cf(&metadata_cf, PREV_HASH_KEY)?
            .map(|bytes| String::from_utf8(bytes).unwrap())
            .unwrap_or_else(|| "0".repeat(64));

        debug!(prev_hash = %hash, "Retrieved previous hash");
        Ok(hash)
    }

    /// Gets offset for a given event hash (deduplication check).
    #[instrument(skip(self), fields(hash))]
    pub fn get_offset_by_hash(&self, hash: &str) -> Result<Option<u64>, EventError> {
        debug!(hash = %hash, "Looking up offset by hash");

        let index_cf = self
            .db
            .cf_handle(CF_INDEX)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_INDEX.to_string()))?;

        let offset = self
            .db
            .get_cf(&index_cf, hash.as_bytes())?
            .map(|bytes| {
                let arr: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
                    EventError::InvalidData("Invalid offset bytes in index".to_string())
                })?;
                Ok::<u64, EventError>(u64::from_be_bytes(arr))
            })
            .transpose()?;

        if offset.is_some() {
            debug!(hash = %hash, offset = ?offset, "Found offset for hash");
        } else {
            debug!(hash = %hash, "No offset found for hash");
        }

        Ok(offset)
    }

    /// Returns an iterator over all events in the store.
    pub fn iter_events(&self) -> Result<RocksDbEventIterator<'_>, EventError> {
        let events_cf = self
            .db
            .cf_handle(CF_EVENTS)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_EVENTS.to_string()))?;

        let iter = self.db.iterator_cf(&events_cf, IteratorMode::Start);

        Ok(RocksDbEventIterator {
            iter,
            _db: Arc::clone(&self.db),
        })
    }

    /// Returns the number of events in the store.
    #[instrument(skip(self))]
    pub fn event_count(&self) -> Result<usize, EventError> {
        let events_cf = self
            .db
            .cf_handle(CF_EVENTS)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_EVENTS.to_string()))?;

        let count = self.db.iterator_cf(&events_cf, IteratorMode::Start).count();
        debug!(count = count, "Retrieved event count");
        Ok(count)
    }

    /// Returns an iterator over events starting from a given offset.
    #[instrument(skip(self))]
    pub fn iter_from(&self, start_offset: u64) -> Result<RocksDbEventIterator<'_>, EventError> {
        debug!(start_offset = start_offset, "Creating event iterator");

        let events_cf = self
            .db
            .cf_handle(CF_EVENTS)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_EVENTS.to_string()))?;

        let iter = self.db.iterator_cf(
            &events_cf,
            IteratorMode::From(&start_offset.to_be_bytes(), Direction::Forward),
        );

        Ok(RocksDbEventIterator {
            iter,
            _db: Arc::clone(&self.db),
        })
    }

    /// Returns an iterator over a range of events [start, end).
    #[instrument(skip(self))]
    pub fn iter_range(
        &self,
        start_offset: u64,
        end_offset: u64,
    ) -> Result<RocksDbEventRangeIterator<'_>, EventError> {
        debug!(
            start_offset = start_offset,
            end_offset = end_offset,
            "Creating range iterator"
        );

        let events_cf = self
            .db
            .cf_handle(CF_EVENTS)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_EVENTS.to_string()))?;

        let iter = self.db.iterator_cf(
            &events_cf,
            IteratorMode::From(&start_offset.to_be_bytes(), Direction::Forward),
        );

        Ok(RocksDbEventRangeIterator {
            iter,
            end_offset,
            _db: Arc::clone(&self.db),
        })
    }
}

/// An iterator for traversing events in the RocksDB store.
pub struct RocksDbEventIterator<'a> {
    iter: DBIteratorWithThreadMode<'a, DB>,
    _db: Arc<DB>,
}

impl<'a> Iterator for RocksDbEventIterator<'a> {
    type Item = Result<(u64, Event), EventError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|result| {
            result
                .map_err(EventError::from)
                .and_then(|(offset_bytes, event_bytes)| {
                    let offset =
                        u64::from_be_bytes(offset_bytes.as_ref().try_into().map_err(|_| {
                            EventError::InvalidData("Invalid offset bytes".to_string())
                        })?);

                    let event: Event =
                        serde_json::from_slice(&event_bytes).map_err(EventError::Json)?;

                    Ok((offset, event))
                })
        })
    }
}

/// An iterator for traversing a range of events in the RocksDB store.
pub struct RocksDbEventRangeIterator<'a> {
    iter: DBIteratorWithThreadMode<'a, DB>,
    end_offset: u64,
    _db: Arc<DB>,
}

impl<'a> Iterator for RocksDbEventRangeIterator<'a> {
    type Item = Result<(u64, Event), EventError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().and_then(|result| {
            match result {
                Ok((offset_bytes, event_bytes)) => {
                    let offset = match offset_bytes.as_ref().try_into() {
                        Ok(arr) => u64::from_be_bytes(arr),
                        Err(_) => {
                            return Some(Err(EventError::InvalidData(
                                "Invalid offset bytes".to_string(),
                            )));
                        }
                    };

                    // Check if we've reached the end of the range
                    if offset >= self.end_offset {
                        return None;
                    }

                    let event_result = serde_json::from_slice::<Event>(&event_bytes)
                        .map_err(EventError::Json)
                        .map(|event| (offset, event));

                    Some(event_result)
                }
                Err(e) => Some(Err(EventError::from(e))),
            }
        })
    }
}
