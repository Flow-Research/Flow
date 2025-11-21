use event::{
    types::{DottedVersionVector, EventPayload},
    EventStoreConfig, EventValidator, RocksDbEventStore, SignedPayload,
};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TestPayload {
    message: String,
    count: u64,
}

impl EventPayload for TestPayload {
    const TYPE: &'static str = "test.event";
    const VERSION: u32 = 1;
}

fn create_test_store() -> (RocksDbEventStore, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let validator = EventValidator::new();

    // Register test schema
    let schema = serde_json::json!({
        "title": "test.event",
        "version": 1,
        "type": "object",
        "properties": {
            "message": { "type": "string" },
            "count": { "type": "number" }
        },
        "required": ["message", "count"]
    });
    validator.register_schema(schema).unwrap();

    let config = EventStoreConfig::default();
    let store = RocksDbEventStore::new(
        temp_dir.path().join("events"),
        "test_stream".to_string(),
        "test_space".to_string(),
        validator,
        config,
    )
    .expect("Failed to create store");

    (store, temp_dir)
}

fn create_signed_payload(message: &str, count: u64) -> SignedPayload<TestPayload> {
    SignedPayload {
        payload: TestPayload {
            message: message.to_string(),
            count,
        },
        signer_did: "did:test:alice".to_string(),
        signature: "fake_signature_base64".to_string(),
        signature_type: "Ed25519".to_string(),
    }
}

#[test]
fn test_create_store() {
    let (store, _temp) = create_test_store();
    assert_eq!(store.event_count().unwrap(), 0);
}

#[test]
fn test_append_single_event() {
    let (store, _temp) = create_test_store();

    let payload = create_signed_payload("Hello", 1);
    let event = store.append(payload).expect("Failed to append event");

    assert_eq!(event.event_type, "test.event");
    assert_eq!(event.schema_version, 1);
    assert_eq!(event.signer, "did:test:alice");
    assert_eq!(event.sig_type, "Ed25519");
    assert_eq!(store.event_count().unwrap(), 1);
}

#[test]
fn test_append_multiple_events() {
    let (store, _temp) = create_test_store();

    for i in 0..10 {
        let payload = create_signed_payload(&format!("Message {}", i), i);
        store.append(payload).expect("Failed to append event");
    }

    assert_eq!(store.event_count().unwrap(), 10);
}

#[test]
fn test_get_event_by_offset() {
    let (store, _temp) = create_test_store();

    let payload1 = create_signed_payload("First", 1);
    store.append(payload1).unwrap();

    let payload2 = create_signed_payload("Second", 2);
    let event2 = store.append(payload2).unwrap();

    let retrieved = store.get_event(1).unwrap().expect("Event not found");
    assert_eq!(retrieved.id, event2.id);
    assert_eq!(retrieved.signer, "did:test:alice");
}

#[test]
fn test_get_nonexistent_event() {
    let (store, _temp) = create_test_store();

    let result = store.get_event(999).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_get_head_offset() {
    let (store, _temp) = create_test_store();

    // Empty store should error
    assert!(store.get_head_offset().is_err());

    // After appending events
    for i in 0..5 {
        let payload = create_signed_payload(&format!("Message {}", i), i);
        store.append(payload).unwrap();
    }

    assert_eq!(store.get_head_offset().unwrap(), 4);
}

#[test]
fn test_causality_tracking() {
    let (store, _temp) = create_test_store();

    let payload = create_signed_payload("Test", 1);
    store.append(payload).unwrap();

    let causality = store.get_causality().unwrap();
    assert_eq!(causality.clocks.get("did:test:alice"), Some(&1));

    // Append another event from same actor
    let payload2 = create_signed_payload("Test 2", 2);
    store.append(payload2).unwrap();

    let causality2 = store.get_causality().unwrap();
    assert_eq!(causality2.clocks.get("did:test:alice"), Some(&2));
}

#[test]
fn test_hash_chain() {
    let (store, _temp) = create_test_store();

    let payload1 = create_signed_payload("First", 1);
    let event1 = store.append(payload1).unwrap();

    let payload2 = create_signed_payload("Second", 2);
    let event2 = store.append(payload2).unwrap();

    // Verify hash chain
    let hash1 = event1.compute_hash().unwrap();
    assert_eq!(event2.prev_hash, hash1);
}

#[test]
fn test_prev_hash_genesis() {
    let (store, _temp) = create_test_store();

    let payload = create_signed_payload("Genesis", 1);
    let event = store.append(payload).unwrap();

    // First event should have genesis prev_hash
    assert_eq!(event.prev_hash, "0".repeat(64));
}

#[test]
fn test_get_offset_by_hash() {
    let (store, _temp) = create_test_store();

    let payload = create_signed_payload("Test", 1);
    let event = store.append(payload).unwrap();

    let hash = event.compute_hash().unwrap();
    let offset = store.get_offset_by_hash(&hash).unwrap();

    assert_eq!(offset, Some(0));
}

#[test]
fn test_hash_index_deduplication() {
    let (store, _temp) = create_test_store();

    for i in 0..5 {
        let payload = create_signed_payload(&format!("Message {}", i), i);
        store.append(payload).unwrap();
    }

    // All events should be indexed
    for offset in 0..5 {
        let event = store.get_event(offset).unwrap().unwrap();
        let hash = event.compute_hash().unwrap();
        let found_offset = store.get_offset_by_hash(&hash).unwrap();
        assert_eq!(found_offset, Some(offset));
    }
}

#[test]
fn test_iter_events() {
    let (store, _temp) = create_test_store();

    for i in 0..10 {
        let payload = create_signed_payload(&format!("Message {}", i), i);
        store.append(payload).unwrap();
    }

    let events: Vec<_> = store
        .iter_events()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(events.len(), 10);
    assert_eq!(events[0].0, 0); // First offset is 0
    assert_eq!(events[9].0, 9); // Last offset is 9
}

#[test]
fn test_iter_from_offset() {
    let (store, _temp) = create_test_store();

    for i in 0..10 {
        let payload = create_signed_payload(&format!("Message {}", i), i);
        store.append(payload).unwrap();
    }

    let events: Vec<_> = store
        .iter_from(5)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(events.len(), 5);
    assert_eq!(events[0].0, 5);
    assert_eq!(events[4].0, 9);
}

#[test]
fn test_iter_range() {
    let (store, _temp) = create_test_store();

    for i in 0..10 {
        let payload = create_signed_payload(&format!("Message {}", i), i);
        store.append(payload).unwrap();
    }

    let events: Vec<_> = store
        .iter_range(3, 7)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(events.len(), 4);
    assert_eq!(events[0].0, 3);
    assert_eq!(events[3].0, 6);
}

#[test]
fn test_empty_signature_rejected() {
    let (store, _temp) = create_test_store();

    let mut payload = create_signed_payload("Test", 1);
    payload.signature = "".to_string();

    let result = store.append(payload);
    assert!(result.is_err());
}

#[test]
fn test_empty_signature_type_rejected() {
    let (store, _temp) = create_test_store();

    let mut payload = create_signed_payload("Test", 1);
    payload.signature_type = "".to_string();

    let result = store.append(payload);
    assert!(result.is_err());
}

#[test]
fn test_concurrent_appends() {
    use std::sync::Arc;
    use std::thread;

    let (store, _temp) = create_test_store();
    let store = Arc::new(store);

    let mut handles = vec![];
    for i in 0..10 {
        let store_clone = Arc::clone(&store);
        let handle = thread::spawn(move || {
            let payload = create_signed_payload(&format!("Message {}", i), i);
            store_clone.append(payload).expect("Failed to append");
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert_eq!(store.event_count().unwrap(), 10);
}

#[test]
fn test_persistence_across_reopens() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path = temp_dir.path().join("events");

    // First store instance
    {
        let validator = EventValidator::new();
        let schema = serde_json::json!({
            "title": "test.event",
            "version": 1,
            "type": "object",
            "properties": {
                "message": { "type": "string" },
                "count": { "type": "number" }
            },
            "required": ["message", "count"]
        });
        validator.register_schema(schema).unwrap();

        let config = EventStoreConfig::default();
        let store = RocksDbEventStore::new(
            &path,
            "test_stream".to_string(),
            "test_space".to_string(),
            validator,
            config,
        )
        .unwrap();

        for i in 0..5 {
            let payload = create_signed_payload(&format!("Message {}", i), i);
            store.append(payload).unwrap();
        }
    }

    // Second store instance - should load existing data
    {
        let validator = EventValidator::new();
        let schema = serde_json::json!({
            "title": "test.event",
            "version": 1,
            "type": "object",
            "properties": {
                "message": { "type": "string" },
                "count": { "type": "number" }
            },
            "required": ["message", "count"]
        });
        validator.register_schema(schema).unwrap();

        let config = EventStoreConfig::default();
        let store = RocksDbEventStore::new(
            &path,
            "test_stream".to_string(),
            "test_space".to_string(),
            validator,
            config,
        )
        .unwrap();

        assert_eq!(store.event_count().unwrap(), 5);
        assert_eq!(store.get_head_offset().unwrap(), 4);
    }
}

#[test]
fn test_config_from_env() {
    std::env::set_var("EVENT_STORE_MAX_OPEN_FILES", "500");
    std::env::set_var("EVENT_STORE_ENABLE_COMPRESSION", "false");

    let config = EventStoreConfig::from_env();

    assert_eq!(config.max_open_files, 500);
    assert_eq!(config.enable_compression, false);

    // Cleanup
    std::env::remove_var("EVENT_STORE_MAX_OPEN_FILES");
    std::env::remove_var("EVENT_STORE_ENABLE_COMPRESSION");
}

#[test]
fn test_multiple_actors_causality() {
    let (store, _temp) = create_test_store();

    // Actor 1
    let mut payload1 = create_signed_payload("Actor 1", 1);
    payload1.signer_did = "did:test:actor1".to_string();
    store.append(payload1).unwrap();

    // Actor 2
    let mut payload2 = create_signed_payload("Actor 2", 2);
    payload2.signer_did = "did:test:actor2".to_string();
    store.append(payload2).unwrap();

    let causality = store.get_causality().unwrap();
    assert_eq!(causality.clocks.get("did:test:actor1"), Some(&1));
    assert_eq!(causality.clocks.get("did:test:actor2"), Some(&1));
}
