#[path = "fixtures/mod.rs"]
mod fixtures;

use chrono::TimeZone;
use event::{store::SignedPayload, Event, EventStore};
use fixtures::*;
use proptest::prelude::*;
use tempfile::TempDir;

// Generate arbitrary test payloads
fn arb_test_payload() -> impl Strategy<Value = TestPayload> {
    (any::<String>(), any::<i32>()).prop_map(|(message, count)| TestPayload { message, count })
}

// Generate arbitrary DIDs
fn arb_did() -> impl Strategy<Value = String> {
    prop::string::string_regex("did:test:[a-z]{5,10}").unwrap()
}

proptest! {
    #[test]
    fn prop_event_hash_deterministic(
        message in any::<String>(),
        count in any::<i32>(),
    ) {
        let event1 = create_event_with_payload(&message, count);
        let event2 = create_event_with_payload(&message, count);

        let hash1 = event1.compute_hash().unwrap();
        let hash2 = event2.compute_hash().unwrap();

        prop_assert_eq!(hash1, hash2);
    }

    #[test]
    fn prop_event_hash_changes_with_payload(
        message1 in any::<String>(),
        message2 in any::<String>(),
        count in any::<i32>(),
    ) {
        prop_assume!(message1 != message2);

        let event1 = create_event_with_payload(&message1, count);
        let event2 = create_event_with_payload(&message2, count);

        let hash1 = event1.compute_hash().unwrap();
        let hash2 = event2.compute_hash().unwrap();

        prop_assert_ne!(hash1, hash2);
    }

    #[test]
    fn prop_event_hash_ignores_signature(
        message in any::<String>(),
        count in any::<i32>(),
        sig1 in any::<String>(),
        sig2 in any::<String>(),
    ) {
        prop_assume!(sig1 != sig2);

        let mut event1 = create_event_with_payload(&message, count);
        let mut event2 = create_event_with_payload(&message, count);

        event1.sig = sig1;
        event2.sig = sig2;

        let hash1 = event1.compute_hash().unwrap();
        let hash2 = event2.compute_hash().unwrap();

        prop_assert_eq!(hash1, hash2);
    }

    #[test]
    fn prop_dvv_increment_monotonic(actor in arb_did(), count in 1..100usize) {
        let mut dvv = event::types::DottedVersionVector::default();

        for _ in 0..count {
            dvv.increment(&actor);
        }

        let expected_count = count as u64;
        prop_assert_eq!(dvv.clocks.get(&actor), Some(&expected_count));
    }

    #[test]
    fn prop_dvv_precedes_transitive(
        actor in arb_did(),
        n1 in 1..10u64,
        n2 in 10..20u64,
        n3 in 20..30u64,
    ) {
        let mut dvv1 = event::types::DottedVersionVector::default();
        let mut dvv2 = event::types::DottedVersionVector::default();
        let mut dvv3 = event::types::DottedVersionVector::default();

        for _ in 0..n1 {
            dvv1.increment(&actor);
        }
        for _ in 0..n2 {
            dvv2.increment(&actor);
        }
        for _ in 0..n3 {
            dvv3.increment(&actor);
        }

        prop_assert!(dvv1.precedes(&dvv2));
        prop_assert!(dvv2.precedes(&dvv3));
        prop_assert!(dvv1.precedes(&dvv3)); // Transitivity
    }

    #[test]
    fn prop_store_preserves_order(payloads in prop::collection::vec(arb_test_payload(), 1..50)) {
        let temp_dir = TempDir::new().unwrap();
        let validator = create_test_validator();
        let store = EventStore::new(
            temp_dir.path(),
            "prop_stream".to_string(),
            "prop_space".to_string(),
            validator,
        )
        .unwrap();

        // Append all payloads
        for payload in &payloads {
            let signed = SignedPayload {
                payload: payload.clone(),
                signer_did: test_did("alice"),
                signature: fake_signature(&payload.message),
                signature_type: "Ed25519".to_string(),
            };
            store.append(signed).unwrap();
        }

        // Retrieve and verify order
        let events: Vec<Event> = store.iter_events()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        prop_assert_eq!(events.len(), payloads.len());

        for (i, (event, payload)) in events.iter().zip(payloads.iter()).enumerate() {
            let retrieved_message = event.payload.get("message").and_then(|v| v.as_str()).unwrap();
            prop_assert_eq!(retrieved_message, &payload.message, "Event at index {} has wrong message", i);
        }
    }

    #[test]
    fn prop_hash_chain_always_valid(payloads in prop::collection::vec(arb_test_payload(), 2..20)) {
        let temp_dir = TempDir::new().unwrap();
        let validator = create_test_validator();
        let store = EventStore::new(
            temp_dir.path(),
            "prop_chain_stream".to_string(),
            "prop_chain_space".to_string(),
            validator,
        )
        .unwrap();

        for payload in &payloads {
            let signed = SignedPayload {
                payload: payload.clone(),
                signer_did: test_did("alice"),
                signature: fake_signature(&payload.message),
                signature_type: "Ed25519".to_string(),
            };
            store.append(signed).unwrap();
        }

        let events: Vec<Event> = store.iter_events()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        prop_assert!(Event::verify_chain_sequence(&events).unwrap());
    }

    #[test]
    fn prop_canonical_bytes_excludes_signature(
        payload in arb_test_payload(),
        sig in any::<String>(),
        sig_type in any::<String>(),
    ) {
        let mut event = create_event_with_payload(&payload.message, payload.count);
        event.sig = sig.clone();
        event.sig_type = sig_type.clone();

        let canonical = event.canonical_bytes().unwrap();

        // Parse canonical JSON and assert signature keys are absent
        let v: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
        let obj = v.as_object().unwrap();
        prop_assert!(!obj.contains_key("sig"));
        prop_assert!(!obj.contains_key("sig_type"));

        // Optional: also assert the key names are not present in the string
        let s = String::from_utf8(canonical).unwrap();
        prop_assert!(!s.contains("\"sig\""));
        prop_assert!(!s.contains("\"sig_type\""));
    }
}

// Helper functions

fn create_event_with_payload(message: &str, count: i32) -> Event {
    use chrono::Utc;
    use serde_json::json;
    use ulid::Ulid;

    Event {
        id: Ulid::from_bytes([0; 16]),
        event_type: "TestEvent".to_string(),
        schema_version: 1,
        payload: json!({ "message": message, "count": count }),
        ts: Utc.timestamp_opt(0, 0).single().unwrap(),
        causality: event::types::DottedVersionVector::default(),
        prev_hash: "0".repeat(64),
        signer: test_did("alice"),
        sig: "test_sig".to_string(),
        sig_type: "Ed25519".to_string(),
        trust_refs: vec![],
        redactable: false,
    }
}
