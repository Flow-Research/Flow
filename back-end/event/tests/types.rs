use chrono::Utc;
use event::types::{DottedVersionVector, Event};
use serde_json::json;
use ulid::Ulid;

#[test]
fn test_dotted_version_vector_default() {
    let dvv = DottedVersionVector::default();
    assert!(dvv.clocks.is_empty());
    assert!(dvv.dots.is_empty());
}

#[test]
fn test_dvv_increment_single_actor() {
    let mut dvv = DottedVersionVector::default();
    let actor = "did:key:alice";

    dvv.increment(actor);

    assert_eq!(dvv.clocks.get(actor), Some(&1));
    assert_eq!(dvv.dots.len(), 1);
    assert!(dvv.dots.contains(&(actor.to_string(), 1)));
}

#[test]
fn test_dvv_increment_multiple_times() {
    let mut dvv = DottedVersionVector::default();
    let actor = "did:key:alice";

    dvv.increment(actor);
    dvv.increment(actor);
    dvv.increment(actor);

    assert_eq!(dvv.clocks.get(actor), Some(&3));
    // Dots should only contain the latest
    assert_eq!(dvv.dots.len(), 1);
    assert!(dvv.dots.contains(&(actor.to_string(), 3)));
}

#[test]
fn test_dvv_increment_multiple_actors() {
    let mut dvv = DottedVersionVector::default();

    dvv.increment("did:key:alice");
    dvv.increment("did:key:bob");
    dvv.increment("did:key:alice");

    assert_eq!(dvv.clocks.get("did:key:alice"), Some(&2));
    assert_eq!(dvv.clocks.get("did:key:bob"), Some(&1));
}

#[test]
fn test_dvv_precedes_same() {
    let dvv1 = DottedVersionVector::default();
    let dvv2 = DottedVersionVector::default();

    assert!(dvv1.precedes(&dvv2));
    assert!(dvv2.precedes(&dvv1));
}

#[test]
fn test_dvv_precedes_simple() {
    let mut dvv1 = DottedVersionVector::default();
    dvv1.increment("did:key:alice");

    let mut dvv2 = DottedVersionVector::default();
    dvv2.increment("did:key:alice");
    dvv2.increment("did:key:alice");

    assert!(dvv1.precedes(&dvv2));
    assert!(!dvv2.precedes(&dvv1));
}

#[test]
fn test_dvv_concurrent() {
    let mut dvv1 = DottedVersionVector::default();
    dvv1.increment("did:key:alice");

    let mut dvv2 = DottedVersionVector::default();
    dvv2.increment("did:key:bob");

    assert!(dvv1.concurrent(&dvv2));
    assert!(dvv2.concurrent(&dvv1));
}

#[test]
fn test_dvv_merge() {
    let mut dvv1 = DottedVersionVector::default();
    dvv1.increment("did:key:alice");
    dvv1.increment("did:key:alice");

    let mut dvv2 = DottedVersionVector::default();
    dvv2.increment("did:key:bob");
    dvv2.increment("did:key:alice"); // alice at 1

    dvv1.merge(&dvv2);

    // Should take maximum clock values
    assert_eq!(dvv1.clocks.get("did:key:alice"), Some(&2));
    assert_eq!(dvv1.clocks.get("did:key:bob"), Some(&1));

    // Should merge dots
    assert!(dvv1.dots.len() >= 1);
}

#[test]
fn test_dvv_serialization() {
    let mut dvv = DottedVersionVector::default();
    dvv.increment("did:key:alice");
    dvv.increment("did:key:bob");

    let json = serde_json::to_string(&dvv).unwrap();
    let deserialized: DottedVersionVector = serde_json::from_str(&json).unwrap();

    assert_eq!(dvv, deserialized);
}

#[test]
fn test_event_canonical_bytes_excludes_signature() {
    let event = create_test_event();

    let canonical = event.canonical_bytes().unwrap();
    let canonical_str = String::from_utf8(canonical).unwrap();

    // Should contain these fields
    assert!(canonical_str.contains("payload"));

    // Should NOT contain signature fields
    assert!(!canonical_str.contains("test_signature"));
    assert!(!canonical_str.contains("Ed25519"));
}

#[test]
fn test_event_canonical_bytes_deterministic() {
    let event = create_test_event();

    let bytes1 = event.canonical_bytes().unwrap();
    let bytes2 = event.canonical_bytes().unwrap();

    assert_eq!(bytes1, bytes2);
}

#[test]
fn test_event_compute_hash() {
    let event = create_test_event();

    let hash1 = event.compute_hash().unwrap();
    let hash2 = event.compute_hash().unwrap();

    // Should be deterministic
    assert_eq!(hash1, hash2);

    // Should be hex SHA-256 (64 characters)
    assert_eq!(hash1.len(), 64);
    assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_event_compute_hash_changes_with_content() {
    let event1 = create_test_event();
    let mut event2 = create_test_event();

    event2.payload = json!({"different": "payload"});

    let hash1 = event1.compute_hash().unwrap();
    let hash2 = event2.compute_hash().unwrap();

    assert_ne!(hash1, hash2);
}

#[test]
fn test_event_compute_hash_ignores_signature() {
    let base = create_test_event();
    let mut event1 = base.clone();
    let mut event2 = base.clone();

    event1.sig = "signature_a".to_string();
    event2.sig = "signature_b".to_string();

    let hash1 = event1.compute_hash().unwrap();
    let hash2 = event2.compute_hash().unwrap();

    // Hashes should be the same despite different signatures
    assert_eq!(hash1, hash2);
}

#[test]
fn test_event_verify_chain_valid() {
    let event1 = create_test_event();
    let hash1 = event1.compute_hash().unwrap();

    let mut event2 = create_test_event();
    event2.prev_hash = hash1;

    assert!(event2.verify_chain(&event1).unwrap());
}

#[test]
fn test_event_verify_chain_invalid() {
    let event1 = create_test_event();

    let mut event2 = create_test_event();
    event2.prev_hash = "wrong_hash".repeat(64);

    assert!(!event2.verify_chain(&event1).unwrap());
}

#[test]
fn test_event_verify_chain_sequence_valid() {
    let events = create_event_chain(5);
    assert!(Event::verify_chain_sequence(&events).unwrap());
}

#[test]
fn test_event_verify_chain_sequence_broken() {
    let mut events = create_event_chain(5);

    // Break the chain at position 2
    events[2].prev_hash = "broken_hash".repeat(64);

    assert!(!Event::verify_chain_sequence(&events).unwrap());
}

#[test]
fn test_event_verify_chain_sequence_empty() {
    let events: Vec<Event> = vec![];
    assert!(Event::verify_chain_sequence(&events).unwrap());
}

#[test]
fn test_event_verify_chain_sequence_single() {
    let events = vec![create_test_event()];
    assert!(Event::verify_chain_sequence(&events).unwrap());
}

#[test]
fn test_event_serialization_roundtrip() {
    let event = create_test_event();

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: Event = serde_json::from_str(&json).unwrap();

    assert_eq!(event.id, deserialized.id);
    assert_eq!(event.signer, deserialized.signer);
    assert_eq!(event.sig, deserialized.sig);
}

// Helper functions

fn create_test_event() -> Event {
    Event {
        id: Ulid::new(),
        event_type: "TestEvent".to_string(),
        schema_version: 1,
        payload: json!({"message": "test", "count": 42}),
        ts: Utc::now(),
        causality: DottedVersionVector::default(),
        prev_hash: "0".repeat(64),
        signer: "did:key:test".to_string(),
        sig: "test_signature".to_string(),
        sig_type: "Ed25519".to_string(),
        trust_refs: vec![],
        redactable: false,
    }
}

fn create_event_chain(count: usize) -> Vec<Event> {
    let mut events = Vec::new();
    let mut prev_hash = "0".repeat(64);

    for i in 0..count {
        let mut event = create_test_event();
        event.payload = json!({"index": i});
        event.prev_hash = prev_hash.clone();

        prev_hash = event.compute_hash().unwrap();
        events.push(event);
    }

    events
}
