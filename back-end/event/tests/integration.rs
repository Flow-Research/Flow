#[path = "fixtures/mod.rs"]
mod fixtures;

use event::{
    store::SignedPayload, Dispatcher, Event, EventError, EventHandler, EventStoreConfig,
    PersistentSubscription, RocksDbEventStore,
};
use fixtures::*;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[test]
fn test_end_to_end_event_flow() {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    // 1. Create event store
    let store = RocksDbEventStore::new(
        temp_dir.path().join("events"),
        "integration_stream".to_string(),
        "integration_space".to_string(),
        validator,
        config,
    )
    .unwrap();

    // 2. Append events from multiple actors
    for i in 0..5 {
        let payload = SignedPayload {
            payload: TestPayload {
                message: format!("Alice message {}", i),
                count: i,
            },
            signer_did: test_did("alice"),
            signature: fake_signature(&format!("alice_{}", i)),
            signature_type: "Ed25519".to_string(),
        };
        store.append(payload).unwrap();
    }

    for i in 0..5 {
        let payload = SignedPayload {
            payload: TestPayload {
                message: format!("Bob message {}", i),
                count: i + 100,
            },
            signer_did: test_did("bob"),
            signature: fake_signature(&format!("bob_{}", i)),
            signature_type: "Ed25519".to_string(),
        };
        store.append(payload).unwrap();
    }

    // 3. Verify hash chain integrity
    let events: Vec<Event> = store
        .iter_events()
        .unwrap()
        .map(|r| r.map(|(_, e)| e))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(events.len(), 10);
    assert!(Event::verify_chain_sequence(&events).unwrap());

    // 4. Verify causality tracking
    let causality = store.get_causality().unwrap();
    assert_eq!(causality.clocks.get(&test_did("alice")), Some(&5));
    assert_eq!(causality.clocks.get(&test_did("bob")), Some(&5));

    // 5. Process events with handlers
    let handler = CollectingHandler::new();
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    let dispatcher = Dispatcher::new(&store);
    dispatcher.poll(&mut subscription).unwrap();

    assert_eq!(handler.event_count(), 10);
}

#[test]
fn test_store_persistence_and_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("events");

    let original_events_count = 20;

    // Phase 1: Create store and populate
    {
        let validator = create_test_validator();
        let config = EventStoreConfig::default();
        let store = RocksDbEventStore::new(
            &path,
            "persistent_stream".to_string(),
            "persistent_space".to_string(),
            validator,
            config,
        )
        .unwrap();

        for i in 0..original_events_count {
            let payload = SignedPayload {
                payload: TestPayload {
                    message: format!("Persistent message {}", i),
                    count: i,
                },
                signer_did: test_did("alice"),
                signature: fake_signature(&format!("sig_{}", i)),
                signature_type: "Ed25519".to_string(),
            };
            store.append(payload).unwrap();
        }

        assert_eq!(store.event_count().unwrap(), original_events_count as usize);
    }

    // Phase 2: Reopen store and verify
    {
        let validator = create_test_validator();
        let config = EventStoreConfig::default();
        let store = RocksDbEventStore::new(
            &path,
            "persistent_stream".to_string(),
            "persistent_space".to_string(),
            validator,
            config,
        )
        .unwrap();

        assert_eq!(store.event_count().unwrap(), original_events_count as usize);

        // Verify hash chain is intact
        let events: Vec<Event> = store
            .iter_events()
            .unwrap()
            .map(|r| r.map(|(_, e)| e))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(Event::verify_chain_sequence(&events).unwrap());

        // Add more events
        for i in original_events_count..original_events_count + 10 {
            let payload = SignedPayload {
                payload: TestPayload {
                    message: format!("New message {}", i),
                    count: i,
                },
                signer_did: test_did("alice"),
                signature: fake_signature(&format!("sig_{}", i)),
                signature_type: "Ed25519".to_string(),
            };
            store.append(payload).unwrap();
        }

        assert_eq!(
            store.event_count().unwrap(),
            (original_events_count + 10) as usize
        );
    }
}

#[test]
fn test_concurrent_readers_single_writer() {
    use std::thread;

    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("events");

    // Open one shared store and reuse it across threads
    let validator = create_test_validator();
    let config = EventStoreConfig::default();
    let store = Arc::new(
        RocksDbEventStore::new(
            &path,
            "concurrent_stream".to_string(),
            "concurrent_space".to_string(),
            validator,
            config,
        )
        .unwrap(),
    );

    // Writer thread
    let store_writer = Arc::clone(&store);
    let writer = thread::spawn(move || {
        for i in 0..50 {
            let payload = SignedPayload {
                payload: TestPayload {
                    message: format!("Message {}", i),
                    count: i,
                },
                signer_did: test_did("writer"),
                signature: fake_signature(&format!("sig_{}", i)),
                signature_type: "Ed25519".to_string(),
            };
            store_writer.append(payload).unwrap();
            thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // Give writer time to create some events
    thread::sleep(std::time::Duration::from_millis(100));

    // Reader threads
    let mut readers = vec![];
    for _ in 0..3 {
        let store_reader = Arc::clone(&store);
        let reader = thread::spawn(move || {
            // Read events multiple times
            for _ in 0..5 {
                let _events: Vec<Event> = store_reader
                    .iter_events()
                    .unwrap()
                    .map(|r| r.map(|(_, e)| e))
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        readers.push(reader);
    }

    // Wait for all threads
    writer.join().unwrap();
    for reader in readers {
        reader.join().unwrap();
    }
}

#[test]
fn test_multi_handler_processing() {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = RocksDbEventStore::new(
        temp_dir.path().join("events"),
        "multi_handler_stream".to_string(),
        "multi_handler_space".to_string(),
        validator,
        config,
    )
    .unwrap();

    // Append events
    for i in 0..10 {
        let payload = SignedPayload {
            payload: TestPayload {
                message: format!("Message {}", i),
                count: i,
            },
            signer_did: test_did("alice"),
            signature: fake_signature(&format!("sig_{}", i)),
            signature_type: "Ed25519".to_string(),
        };
        store.append(payload).unwrap();
    }

    // Create multiple handlers
    let handler1 = CollectingHandler::new();
    let handler2 = CountingHandler::new();
    let handler3 = SumHandler::new();

    let mut sub1 =
        PersistentSubscription::new(Arc::clone(store.db()), handler1.clone(), 1000).unwrap();
    let mut sub2 =
        PersistentSubscription::new(Arc::clone(store.db()), handler2.clone(), 1000).unwrap();
    let mut sub3 =
        PersistentSubscription::new(Arc::clone(store.db()), handler3.clone(), 1000).unwrap();

    let dispatcher = Dispatcher::new(&store);

    // Process with all handlers
    dispatcher.poll(&mut sub1).unwrap();
    dispatcher.poll(&mut sub2).unwrap();
    dispatcher.poll(&mut sub3).unwrap();

    // Verify each handler processed events correctly
    assert_eq!(handler1.event_count(), 10);
    assert_eq!(handler2.count(), 10);
    assert_eq!(handler3.sum(), 45); // Sum of 0..10
}

#[test]
fn test_version_migration_scenario() {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = RocksDbEventStore::new(
        temp_dir.path().join("events"),
        "migration_stream".to_string(),
        "migration_space".to_string(),
        validator,
        config,
    )
    .unwrap();

    // Append V1 events
    for i in 0..5 {
        let payload = SignedPayload {
            payload: PayloadV1 {
                name: format!("V1 Name {}", i),
            },
            signer_did: test_did("alice"),
            signature: fake_signature(&format!("v1_{}", i)),
            signature_type: "Ed25519".to_string(),
        };
        store.append(payload).unwrap();
    }

    // Append V2 events
    for i in 0..5 {
        let payload = SignedPayload {
            payload: PayloadV2 {
                name: format!("V2 Name {}", i),
                description: format!("V2 Description {}", i),
            },
            signer_did: test_did("alice"),
            signature: fake_signature(&format!("v2_{}", i)),
            signature_type: "Ed25519".to_string(),
        };
        store.append(payload).unwrap();
    }

    // Verify both versions coexist
    assert_eq!(store.event_count().unwrap(), 10);

    let events: Vec<Event> = store
        .iter_events()
        .unwrap()
        .map(|r| r.map(|(_, e)| e))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let v1_count = events.iter().filter(|e| e.schema_version == 1).count();
    let v2_count = events.iter().filter(|e| e.schema_version == 2).count();

    assert_eq!(v1_count, 5);
    assert_eq!(v2_count, 5);
}

#[test]
fn test_subscription_idempotency_across_restarts() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("events");

    // Phase 1: Create store and append events
    {
        let validator = create_test_validator();
        let config = EventStoreConfig::default();
        let store = RocksDbEventStore::new(
            &path,
            "restart_stream".to_string(),
            "restart_space".to_string(),
            validator,
            config,
        )
        .unwrap();

        for i in 0..10 {
            let payload = SignedPayload {
                payload: TestPayload {
                    message: format!("Message {}", i),
                    count: i,
                },
                signer_did: test_did("alice"),
                signature: fake_signature(&format!("sig_{}", i)),
                signature_type: "Ed25519".to_string(),
            };
            store.append(payload).unwrap();
        }

        // Process first 5 events
        let handler = CountingHandler::new();
        let mut subscription =
            PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

        let events: Vec<_> = store
            .iter_range(0, 5)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        subscription.process_batch(&events).unwrap();

        assert_eq!(handler.count(), 5);
        assert_eq!(subscription.bookmark().unwrap(), 4);
    }

    // Phase 2: Reopen store and continue processing
    {
        let validator = create_test_validator();
        let config = EventStoreConfig::default();
        let store = RocksDbEventStore::new(
            &path,
            "restart_stream".to_string(),
            "restart_space".to_string(),
            validator,
            config,
        )
        .unwrap();

        let handler = CountingHandler::new();
        let mut subscription =
            PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

        // Bookmark should be persisted
        assert_eq!(subscription.bookmark().unwrap(), 4);

        // Poll should only process remaining events
        let dispatcher = Dispatcher::new(&store);
        dispatcher.poll(&mut subscription).unwrap();

        assert_eq!(handler.count(), 5); // Only events 5-9
    }
}

#[test]
fn test_hash_deduplication_index() {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = RocksDbEventStore::new(
        temp_dir.path().join("events"),
        "dedup_stream".to_string(),
        "dedup_space".to_string(),
        validator,
        config,
    )
    .unwrap();

    // Append events
    for i in 0..5 {
        let payload = SignedPayload {
            payload: TestPayload {
                message: format!("Message {}", i),
                count: i,
            },
            signer_did: test_did("alice"),
            signature: fake_signature(&format!("sig_{}", i)),
            signature_type: "Ed25519".to_string(),
        };
        store.append(payload).unwrap();
    }

    // Verify each event can be found by hash
    for offset in 0..5 {
        let event = store.get_event(offset).unwrap().unwrap();
        let hash = event.compute_hash().unwrap();
        let found_offset = store.get_offset_by_hash(&hash).unwrap();

        assert_eq!(found_offset, Some(offset));
    }

    // Non-existent hash should return None
    let fake_hash = "0".repeat(64);
    assert_eq!(store.get_offset_by_hash(&fake_hash).unwrap(), None);
}

// Helper handlers

#[derive(Clone)]
struct CollectingHandler {
    events: Arc<Mutex<Vec<Event>>>,
}

impl CollectingHandler {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn event_count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

impl EventHandler for CollectingHandler {
    fn name(&self) -> &str {
        "collecting_handler"
    }

    fn handle(&mut self, _offset: u64, event: &Event) -> Result<(), EventError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingHandler {
    count: Arc<Mutex<usize>>,
}

impl CountingHandler {
    fn new() -> Self {
        Self {
            count: Arc::new(Mutex::new(0)),
        }
    }

    fn count(&self) -> usize {
        *self.count.lock().unwrap()
    }
}

impl EventHandler for CountingHandler {
    fn name(&self) -> &str {
        "counting_handler"
    }

    fn handle(&mut self, _offset: u64, _event: &Event) -> Result<(), EventError> {
        *self.count.lock().unwrap() += 1;
        Ok(())
    }
}

#[derive(Clone)]
struct SumHandler {
    sum: Arc<Mutex<i32>>,
}

impl SumHandler {
    fn new() -> Self {
        Self {
            sum: Arc::new(Mutex::new(0)),
        }
    }

    fn sum(&self) -> i32 {
        *self.sum.lock().unwrap()
    }
}

impl EventHandler for SumHandler {
    fn name(&self) -> &str {
        "sum_handler"
    }

    fn handle(&mut self, _offset: u64, event: &Event) -> Result<(), EventError> {
        if let Some(count) = event.payload.get("count").and_then(|v| v.as_i64()) {
            *self.sum.lock().unwrap() += count as i32;
        }
        Ok(())
    }
}
