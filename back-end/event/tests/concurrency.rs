#[path = "fixtures/mod.rs"]
mod fixtures;

use event::{store::SignedPayload, EventStoreConfig, RocksDbEventStore};
use fixtures::*;
use serial_test::serial;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

#[test]
#[serial]
fn test_concurrent_appends_single_actor() {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = Arc::new(
        RocksDbEventStore::new(
            temp_dir.path().join("events"),
            "concurrent_stream".to_string(),
            "concurrent_space".to_string(),
            validator,
            config,
        )
        .unwrap(),
    );

    let thread_count = 10;
    let events_per_thread = 10;
    let barrier = Arc::new(Barrier::new(thread_count));

    let mut handles = vec![];

    for thread_id in 0..thread_count {
        let store_clone = Arc::clone(&store);
        let barrier_clone = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            // Wait for all threads to be ready
            barrier_clone.wait();

            for i in 0..events_per_thread {
                let payload = SignedPayload {
                    payload: TestPayload {
                        message: format!("Thread {} message {}", thread_id, i),
                        count: i as i32,
                    },
                    signer_did: test_did("alice"),
                    signature: fake_signature(&format!("{}_{}", thread_id, i)),
                    signature_type: "Ed25519".to_string(),
                };

                store_clone.append(payload).unwrap();
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all events were appended
    let count = store.event_count().unwrap();
    assert_eq!(count, thread_count * events_per_thread);

    // Verify hash chain is valid
    let events: Vec<_> = store
        .iter_events()
        .unwrap()
        .map(|r| r.map(|(_, e)| e))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(event::Event::verify_chain_sequence(&events).unwrap());
}

#[test]
#[serial]
fn test_concurrent_appends_multiple_actors() {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = Arc::new(
        RocksDbEventStore::new(
            temp_dir.path().join("events"),
            "multi_actor_stream".to_string(),
            "multi_actor_space".to_string(),
            validator,
            config,
        )
        .unwrap(),
    );

    let actors = vec!["alice", "bob", "charlie"];
    let events_per_actor = 20;
    let mut handles = vec![];

    for actor in &actors {
        let store_clone = Arc::clone(&store);
        let actor_name = actor.to_string();

        let handle = thread::spawn(move || {
            for i in 0..events_per_actor {
                let payload = SignedPayload {
                    payload: TestPayload {
                        message: format!("{} message {}", actor_name, i),
                        count: i as i32,
                    },
                    signer_did: test_did(&actor_name),
                    signature: fake_signature(&format!("{}_{}", actor_name, i)),
                    signature_type: "Ed25519".to_string(),
                };

                store_clone.append(payload).unwrap();

                // Small delay to increase interleaving
                thread::sleep(std::time::Duration::from_micros(100));
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify total count
    let count = store.event_count().unwrap();
    assert_eq!(count, actors.len() * events_per_actor);

    // Verify causality for each actor
    let causality = store.get_causality().unwrap();
    for actor in &actors {
        assert_eq!(
            causality.clocks.get(&test_did(actor)),
            Some(&(events_per_actor as u64))
        );
    }

    // Verify hash chain
    let events: Vec<_> = store
        .iter_events()
        .unwrap()
        .map(|r| r.map(|(_, e)| e))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(event::Event::verify_chain_sequence(&events).unwrap());
}

#[test]
#[serial]
fn test_concurrent_read_write() {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = Arc::new(
        RocksDbEventStore::new(
            temp_dir.path().join("events"),
            "read_write_stream".to_string(),
            "read_write_space".to_string(),
            validator,
            config,
        )
        .unwrap(),
    );

    let write_count = 50;
    let reader_count = 5;

    // Writer thread
    let store_writer = Arc::clone(&store);
    let writer = thread::spawn(move || {
        for i in 0..write_count {
            let payload = SignedPayload {
                payload: TestPayload {
                    message: format!("Write {}", i),
                    count: i as i32,
                },
                signer_did: test_did("writer"),
                signature: fake_signature(&format!("write_{}", i)),
                signature_type: "Ed25519".to_string(),
            };

            store_writer.append(payload).unwrap();
            thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // Reader threads
    let mut readers = vec![];
    for reader_id in 0..reader_count {
        let store_reader = Arc::clone(&store);

        let reader = thread::spawn(move || {
            let mut last_count = 0;
            for _ in 0..20 {
                let count = store_reader.event_count().unwrap();

                // Count should only increase
                assert!(
                    count >= last_count,
                    "Reader {} saw count decrease",
                    reader_id
                );
                last_count = count;

                // Try to read some events
                if count > 0 {
                    let _events: Vec<_> = store_reader
                        .iter_range(0, count.min(10) as u64)
                        .unwrap()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                }

                thread::sleep(std::time::Duration::from_millis(15));
            }
        });

        readers.push(reader);
    }

    writer.join().unwrap();
    for reader in readers {
        reader.join().unwrap();
    }

    // Final verification
    assert_eq!(store.event_count().unwrap(), write_count);
}

#[test]
#[serial]
fn test_validator_thread_safety() {
    use serde_json::json;
    use std::sync::Arc;

    let validator = Arc::new(create_test_validator());
    let thread_count = 10;
    let validations_per_thread = 100;

    let mut handles = vec![];

    for thread_id in 0..thread_count {
        let validator_clone = Arc::clone(&validator);

        let handle = thread::spawn(move || {
            for i in 0..validations_per_thread {
                let payload = json!({
                    "message": format!("Thread {} validation {}", thread_id, i),
                    "count": i
                });

                let result = validator_clone.validate(&payload, "TestEvent", 1);
                assert!(result.is_ok());
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
#[serial]
fn test_subscription_concurrent_processing() {
    use event::{Dispatcher, EventHandler, PersistentSubscription};
    use std::sync::Mutex;

    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = Arc::new(
        RocksDbEventStore::new(
            temp_dir.path().join("events"),
            "subscription_stream".to_string(),
            "subscription_space".to_string(),
            validator,
            config,
        )
        .unwrap(),
    );

    // Append events
    for i in 0..100 {
        let payload = SignedPayload {
            payload: TestPayload {
                message: format!("Message {}", i),
                count: i as i32,
            },
            signer_did: test_did("alice"),
            signature: fake_signature(&format!("sig_{}", i)),
            signature_type: "Ed25519".to_string(),
        };
        store.append(payload).unwrap();
    }

    // Create multiple handlers
    #[derive(Clone)]
    struct CountHandler {
        count: Arc<Mutex<usize>>,
        name: String,
    }

    impl EventHandler for CountHandler {
        fn name(&self) -> &str {
            &self.name
        }

        fn handle(&mut self, _offset: u64, _event: &event::Event) -> Result<(), event::EventError> {
            *self.count.lock().unwrap() += 1;
            Ok(())
        }
    }

    let handler_count = 5;
    let mut handles = vec![];
    let counts: Vec<_> = (0..handler_count)
        .map(|_| Arc::new(Mutex::new(0)))
        .collect();

    for i in 0..handler_count {
        let store_db = Arc::clone(store.db());
        let store_for_dispatch = Arc::clone(&store);
        let count_clone = Arc::clone(&counts[i]);

        let handle = thread::spawn(move || {
            let handler = CountHandler {
                count: count_clone,
                name: format!("handler_{}", i),
            };

            let mut subscription = PersistentSubscription::new(store_db, handler, 1000).unwrap();

            let dispatcher = Dispatcher::new(&store_for_dispatch);
            dispatcher.poll(&mut subscription).unwrap();
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Each handler should have processed all events
    for count in counts {
        assert_eq!(*count.lock().unwrap(), 100);
    }
}
