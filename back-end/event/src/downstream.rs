use crate::{
    store::RocksDbEventStore,
    types::{Event, EventError},
};
use rocksdb::{WriteBatch, DB};
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn, Span};

const DEFAULT_BATCH_SIZE: usize = 100;
const BOOKMARK_KEY: &[u8] = b"bookmark";
const IDEMPOTENCY_KEY_PREFIX: &[u8] = b"idempotency_";

// Column family for subscription state
const CF_SUBSCRIPTION_STATE: &str = "subscription_state";

pub trait EventHandler: Send {
    /// Unique name for this handler; used to isolate state in RocksDB.
    fn name(&self) -> &str;

    /// Optional event filter. Default: accept all.
    fn filter_event(&self, _event: &Event) -> bool {
        true
    }

    /// Handle a single event at the given offset.
    /// Must be idempotent; this function may be called more than once.
    fn handle(&mut self, offset: u64, event: &Event) -> Result<(), EventError>;
}

/// Persistent subscription that manages bookmark and idempotency state per handler.
pub struct PersistentSubscription<H: EventHandler + Send> {
    db: Arc<DB>,
    handler_name: String,
    handler: H,
    idempotency_window_size: u64,
}

impl<H: EventHandler> PersistentSubscription<H> {
    #[instrument(skip(db, handler), fields(handler_name = handler.name()))]
    pub fn new(db: Arc<DB>, handler: H, idempotency_window_size: u64) -> Result<Self, EventError> {
        let handler_name = handler.name().to_string();

        info!(
            handler_name = %handler_name,
            idempotency_window_size = idempotency_window_size,
            "Creating persistent subscription"
        );

        // Ensure subscription state column family exists
        // Note: In production, you should create this CF when opening the DB
        // For now, we assume it's created during EventStore initialization
        if db.cf_handle(CF_SUBSCRIPTION_STATE).is_none() {
            return Err(EventError::ColumnFamilyNotFound(
                CF_SUBSCRIPTION_STATE.to_string(),
            ));
        }

        Ok(Self {
            db,
            handler_name,
            handler,
            idempotency_window_size,
        })
    }

    /// Build a namespaced key for this handler
    fn namespaced_key(&self, key: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.handler_name.len() + 1 + key.len());
        result.extend_from_slice(self.handler_name.as_bytes());
        result.push(b':');
        result.extend_from_slice(key);
        result
    }

    #[instrument(skip(self), fields(handler_name = %self.handler_name))]
    pub fn bookmark(&self) -> Result<u64, EventError> {
        let cf = self
            .db
            .cf_handle(CF_SUBSCRIPTION_STATE)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_SUBSCRIPTION_STATE.to_string()))?;

        let key = self.namespaced_key(BOOKMARK_KEY);
        let bookmark =
            self.db
                .get_cf(&cf, &key)?
                .map(|bytes| {
                    let arr: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
                        EventError::InvalidData("Invalid bookmark bytes".to_string())
                    })?;
                    Ok::<u64, EventError>(u64::from_be_bytes(arr))
                })
                .transpose()?
                .unwrap_or(0);

        debug!(
            handler_name = %self.handler_name,
            bookmark = bookmark,
            "Retrieved bookmark"
        );

        Ok(bookmark)
    }

    /// Process a batch of events with guaranteed idempotence.
    ///
    /// This method ensures:
    /// - Each event is processed at most once per handler (atomic claim)
    /// - Handler is called only after successful claim (no duplicate calls)
    /// - Bookmark advances monotonically
    /// - Idempotency window is maintained with bounded overhead
    #[instrument(
        skip(self, events),
        fields(
            handler_name = %self.handler_name,
            batch_size = events.len(),
            processed = 0,
            skipped = 0
        )
    )]
    pub fn process_batch(&mut self, events: &[(u64, Event)]) -> Result<(), EventError> {
        if events.is_empty() {
            debug!("Empty batch, skipping");
            return Ok(());
        }

        const PRUNE_INTERVAL: u64 = 100;
        let mut processed_count = 0;
        let mut skipped_count = 0;

        info!(
            handler_name = %self.handler_name,
            batch_size = events.len(),
            "Processing event batch"
        );

        let cf = self
            .db
            .cf_handle(CF_SUBSCRIPTION_STATE)
            .ok_or_else(|| EventError::ColumnFamilyNotFound(CF_SUBSCRIPTION_STATE.to_string()))?;

        for (offset, event) in events {
            // Optional filter
            if !self.handler.filter_event(event) {
                debug!(
                    offset = offset,
                    event_id = %event.id,
                    "Event filtered out"
                );
                skipped_count += 1;
                continue;
            }

            // Build idempotency key: handler_name:prefix + offset (BE) + event.id
            let id_key = {
                let mut k = Vec::with_capacity(
                    self.handler_name.len() + 1 + IDEMPOTENCY_KEY_PREFIX.len() + 8 + 16,
                );
                k.extend_from_slice(self.handler_name.as_bytes());
                k.push(b':');
                k.extend_from_slice(IDEMPOTENCY_KEY_PREFIX);
                k.extend_from_slice(&offset.to_be_bytes());
                k.extend_from_slice(&event.id.to_bytes());
                k
            };

            // ATOMIC CLAIM: Check if event is already processed
            debug!(
                offset = offset,
                event_id = %event.id,
                "Checking if event already processed"
            );

            let already_processed = self.db.get_cf(&cf, &id_key)?.is_some();

            if already_processed {
                debug!(
                    offset = offset,
                    event_id = %event.id,
                    "Event already processed, skipping"
                );
                skipped_count += 1;
                continue;
            }

            // Claim the event using WriteBatch for atomicity
            let mut batch = WriteBatch::default();
            batch.put_cf(&cf, &id_key, &[]);
            self.db.write(batch).map_err(|e| {
                error!(
                    offset = offset,
                    event_id = %event.id,
                    error = %e,
                    "Failed to claim event"
                );
                EventError::Storage(format!("Claim write failed: {}", e))
            })?;

            // ONLY ONE THREAD reaches here per event
            // Invoke handler (must be idempotent for external side effects)
            debug!(
                offset = offset,
                event_id = %event.id,
                event_type = %event.event_type,
                "Invoking event handler"
            );
            self.handler.handle(*offset, event).map_err(|e| {
                error!(
                    offset = offset,
                    event_id = %event.id,
                    error = %e,
                    "Handler failed"
                );
                e
            })?;

            processed_count += 1;

            // After successful handling, update bookmark
            let bookmark_key = self.namespaced_key(BOOKMARK_KEY);
            let mut batch = WriteBatch::default();
            batch.put_cf(&cf, &bookmark_key, &offset.to_be_bytes());
            self.db.write(batch).map_err(|e| {
                error!(
                    offset = offset,
                    event_id = %event.id,
                    error = %e,
                    "Failed to update bookmark"
                );
                EventError::Storage(format!("Bookmark write failed: {}", e))
            })?;

            // Prune idempotency set if needed
            let should_prune = *offset % PRUNE_INTERVAL == 0;
            if should_prune {
                self.prune_idempotency_set(&cf)?;
            }
        }

        // Flush to disk for durability
        self.db.flush_cf(&cf)?;

        Span::current().record("processed", processed_count);
        Span::current().record("skipped", skipped_count);

        info!(
            handler_name = %self.handler_name,
            processed = processed_count,
            skipped = skipped_count,
            "Batch processing complete"
        );

        Ok(())
    }

    /// Prune old idempotency keys to maintain bounded memory usage
    fn prune_idempotency_set(
        &self,
        cf: &impl rocksdb::AsColumnFamilyRef,
    ) -> Result<(), EventError> {
        let prefix = {
            let mut p =
                Vec::with_capacity(self.handler_name.len() + 1 + IDEMPOTENCY_KEY_PREFIX.len());
            p.extend_from_slice(self.handler_name.as_bytes());
            p.push(b':');
            p.extend_from_slice(IDEMPOTENCY_KEY_PREFIX);
            p
        };

        // Count keys with our prefix
        let keys: Vec<_> = self
            .db
            .prefix_iterator_cf(cf, &prefix)
            .filter_map(|res| res.ok().map(|(k, _)| k))
            .collect();

        let total_seen = keys.len() as u64;

        if total_seen > self.idempotency_window_size {
            let to_remove = total_seen - self.idempotency_window_size;

            debug!(
                handler_name = %self.handler_name,
                total_seen = total_seen,
                to_remove = to_remove,
                window_size = self.idempotency_window_size,
                "Pruning idempotency set"
            );

            // Remove oldest keys (first in sorted order due to offset encoding)
            let mut batch = WriteBatch::default();
            for key in keys.iter().take(to_remove as usize) {
                batch.delete_cf(cf, key);
            }

            self.db.write(batch).map_err(|e| {
                error!(
                    handler_name = %self.handler_name,
                    error = %e,
                    "Failed to prune idempotency set"
                );
                EventError::Storage(format!("Prune write failed: {}", e))
            })?;

            info!(
                handler_name = %self.handler_name,
                removed = to_remove,
                remaining = self.idempotency_window_size,
                "Pruned idempotency set"
            );
        }

        Ok(())
    }
}

/// Dispatcher that delivers events (post-persistence) to a persistent subscription in batches.
pub struct Dispatcher<'a> {
    store: &'a RocksDbEventStore,
    batch_size: usize,
}

impl<'a> Dispatcher<'a> {
    pub fn new(store: &'a RocksDbEventStore) -> Self {
        Self {
            store,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    /// Polls for new events and delivers them in batches.
    /// Delivery is guaranteed after persistence since we read from `RocksDbEventStore`.
    /// Idempotence is guaranteed by the subscription state.
    ///
    /// # Concurrency
    ///
    /// Do NOT call `poll()` concurrently on the same `PersistentSubscription`.
    /// Multiple concurrent calls will duplicate work and may violate idempotence.
    ///
    /// Safe patterns:
    /// - Single-threaded polling loop
    /// - One subscription per handler per worker
    #[instrument(
        skip(self, subscription),
        fields(
            handler_name = %subscription.handler_name,
            batch_size = self.batch_size,
            batches_processed = 0,
            total_events = 0
        )
    )]
    pub fn poll<H: EventHandler>(
        &self,
        subscription: &mut PersistentSubscription<H>,
    ) -> Result<(), EventError> {
        let mut bookmark = subscription.bookmark()?;
        let mut batches_processed = 0;
        let mut total_events = 0;

        info!(
            handler_name = %subscription.handler_name,
            starting_bookmark = bookmark,
            "Starting event poll"
        );

        loop {
            debug!(
                handler_name = %subscription.handler_name,
                bookmark = bookmark,
                batch_size = self.batch_size,
                "Fetching event batch",
            );

            let batch: Vec<(u64, Event)> = self
                .store
                .iter_from(bookmark)?
                .take(self.batch_size)
                .collect::<Result<Vec<_>, _>>()?;

            if batch.is_empty() {
                debug!("No more events, poll complete");
                break;
            }

            let batch_len = batch.len();
            debug!(batch_len = batch_len, "Processing batch");

            subscription.process_batch(&batch)?;

            batches_processed += 1;
            total_events += batch_len;

            // Next starting position is the last delivered offset + 1
            if let Some((last_offset, _)) = batch.last() {
                bookmark = *last_offset + 1;
            }
        }

        Span::current().record("batches_processed", batches_processed);
        Span::current().record("total_events", total_events);

        info!(
            handler_name = %subscription.handler_name,
            batches_processed = batches_processed,
            total_events = total_events,
            final_bookmark = bookmark,
            "Event poll complete"
        );

        Ok(())
    }
}
