#[path = "fixtures/mod.rs"]
mod fixtures;

use event::{
    store::SignedPayload, Dispatcher, Event, EventError, EventHandler, EventStoreConfig,
    PersistentSubscription, RocksDbEventStore,
};
use fixtures::*;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// Test handler that collects events
#[derive(Clone)]
struct CollectorHandler {
    name: String,
    collected: Arc<Mutex<Vec<(u64, Event)>>>,
}

impl CollectorHandler {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            collected: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn _get_collected(&self) -> Vec<(u64, Event)> {
        self.collected.lock().unwrap().clone()
    }

    fn count(&self) -> usize {
        self.collected.lock().unwrap().len()
    }
}

impl EventHandler for CollectorHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn handle(&mut self, offset: u64, event: &Event) -> Result<(), EventError> {
        self.collected.lock().unwrap().push((offset, event.clone()));
        Ok(())
    }
}

// Filter handler that only accepts specific event types
#[derive(Clone)]
struct FilteringHandler {
    name: String,
    filter_type: String,
    collected: Arc<Mutex<Vec<(u64, Event)>>>,
}

impl FilteringHandler {
    fn new(name: &str, filter_type: &str) -> Self {
        Self {
            name: name.to_string(),
            filter_type: filter_type.to_string(),
            collected: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn count(&self) -> usize {
        self.collected.lock().unwrap().len()
    }
}

impl EventHandler for FilteringHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn filter_event(&self, event: &Event) -> bool {
        event.event_type == self.filter_type
    }

    fn handle(&mut self, offset: u64, event: &Event) -> Result<(), EventError> {
        self.collected.lock().unwrap().push((offset, event.clone()));
        Ok(())
    }
}

// Failing handler for error testing
#[derive(Clone)]
struct FailingHandler {
    name: String,
    fail_count: Arc<Mutex<usize>>,
    max_failures: usize,
}

impl FailingHandler {
    fn new(name: &str, max_failures: usize) -> Self {
        Self {
            name: name.to_string(),
            fail_count: Arc::new(Mutex::new(0)),
            max_failures,
        }
    }

    fn get_fail_count(&self) -> usize {
        *self.fail_count.lock().unwrap()
    }
}

impl EventHandler for FailingHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn handle(&mut self, _offset: u64, _event: &Event) -> Result<(), EventError> {
        let mut count = self.fail_count.lock().unwrap();
        *count += 1;

        if *count <= self.max_failures {
            Err(EventError::Validation("Intentional failure".to_string()))
        } else {
            Ok(())
        }
    }
}

#[test]
fn test_persistent_subscription_creation() {
    let (store, _temp_dir) = create_test_store();
    let handler = CollectorHandler::new("test_handler");

    let subscription = PersistentSubscription::new(
        Arc::clone(store.db()),
        handler,
        1000, // idempotency window
    );

    assert!(subscription.is_ok());
}

#[test]
fn test_persistent_subscription_bookmark_default() {
    let (store, _temp_dir) = create_test_store();
    let handler = CollectorHandler::new("test_handler");

    let subscription = PersistentSubscription::new(Arc::clone(store.db()), handler, 1000).unwrap();

    let bookmark = subscription.bookmark().unwrap();
    assert_eq!(bookmark, 0);
}

#[test]
fn test_persistent_subscription_process_batch() {
    let (store, _temp_dir) = create_test_store();

    // Append some events
    for i in 0..5 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    // Collect events via subscription
    let handler = CollectorHandler::new("collector");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    let events: Vec<_> = store
        .iter_from(0)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    subscription.process_batch(&events).unwrap();

    assert_eq!(handler.count(), 5);
}

#[test]
fn test_persistent_subscription_idempotency() {
    let (store, _temp_dir) = create_test_store();

    // Append events
    for i in 0..3 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    let handler = CollectorHandler::new("collector");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    let events: Vec<_> = store
        .iter_from(0)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Process batch twice
    subscription.process_batch(&events).unwrap();
    subscription.process_batch(&events).unwrap();

    // Should only process each event once
    assert_eq!(handler.count(), 3);
}

#[test]
fn test_persistent_subscription_bookmark_advances() {
    let (store, _temp_dir) = create_test_store();

    // Append events
    for i in 0..5 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    let handler = CollectorHandler::new("collector");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    let events: Vec<_> = store
        .iter_from(0)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    subscription.process_batch(&events).unwrap();

    // Bookmark should advance to last processed offset
    let bookmark = subscription.bookmark().unwrap();
    assert_eq!(bookmark, 4); // Last offset is 4 (0-indexed, 5 events)
}

#[test]
fn test_persistent_subscription_filter() {
    let (store, _temp_dir) = create_test_store();

    // Append mixed events
    for i in 0..5 {
        store
            .append(create_signed_payload(&format!("Test {}", i), i))
            .unwrap();
    }

    // Handler that filters for TestEvent
    let handler = FilteringHandler::new("filter_handler", "TestEvent");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    let events: Vec<_> = store
        .iter_from(0)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    subscription.process_batch(&events).unwrap();

    // All events should be TestEvent, so all should be processed
    assert_eq!(handler.count(), 5);
}

#[test]
fn test_persistent_subscription_handler_failure() {
    let (store, _temp_dir) = create_test_store();

    store.append(create_signed_payload("Msg", 1)).unwrap();

    let handler = FailingHandler::new("failing_handler", 1);
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    let events: Vec<_> = store
        .iter_from(0)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let result = subscription.process_batch(&events);

    // Should fail
    assert!(result.is_err());
    assert_eq!(handler.get_fail_count(), 1);
}

#[test]
fn test_dispatcher_creation() {
    let (store, _temp_dir) = create_test_store();

    let _dispatcher = Dispatcher::new(&store);

    // Should not panic
    assert!(true);
}

#[test]
fn test_dispatcher_with_batch_size() {
    let (store, _temp_dir) = create_test_store();

    let _dispatcher = Dispatcher::new(&store).with_batch_size(50);

    // Should not panic
    assert!(true);
}

#[test]
fn test_dispatcher_poll_no_events() {
    let (store, _temp_dir) = create_test_store();
    let dispatcher = Dispatcher::new(&store);

    let handler = CollectorHandler::new("collector");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    let result = dispatcher.poll(&mut subscription);

    assert!(result.is_ok());
    assert_eq!(handler.count(), 0);
}

#[test]
fn test_dispatcher_poll_processes_events() {
    let (store, _temp_dir) = create_test_store();

    // Append events
    for i in 0..10 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    let dispatcher = Dispatcher::new(&store);
    let handler = CollectorHandler::new("collector");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    dispatcher.poll(&mut subscription).unwrap();

    assert_eq!(handler.count(), 10);
}

#[test]
fn test_dispatcher_poll_batching() {
    let (store, _temp_dir) = create_test_store();

    // Append many events
    for i in 0..250 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    let dispatcher = Dispatcher::new(&store).with_batch_size(100);
    let handler = CollectorHandler::new("collector");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    dispatcher.poll(&mut subscription).unwrap();

    // Should process all events despite batching
    assert_eq!(handler.count(), 250);
}

#[test]
fn test_dispatcher_poll_incremental() {
    let (store, _temp_dir) = create_test_store();

    // Append initial events
    for i in 0..5 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    let dispatcher = Dispatcher::new(&store);
    let handler = CollectorHandler::new("collector");
    let mut subscription =
        PersistentSubscription::new(Arc::clone(store.db()), handler.clone(), 1000).unwrap();

    // First poll
    dispatcher.poll(&mut subscription).unwrap();
    assert_eq!(handler.count(), 5);

    // Append more events
    for i in 5..10 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    // Second poll should only get new events
    dispatcher.poll(&mut subscription).unwrap();
    assert_eq!(handler.count(), 10);
}

#[test]
fn test_multiple_subscriptions_same_store() {
    let (store, _temp_dir) = create_test_store();

    // Append events
    for i in 0..5 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    // Create two subscriptions with different handlers
    let handler1 = CollectorHandler::new("handler1");
    let mut subscription1 =
        PersistentSubscription::new(Arc::clone(store.db()), handler1.clone(), 1000).unwrap();

    let handler2 = CollectorHandler::new("handler2");
    let mut subscription2 =
        PersistentSubscription::new(Arc::clone(store.db()), handler2.clone(), 1000).unwrap();

    let dispatcher = Dispatcher::new(&store);

    // Poll both
    dispatcher.poll(&mut subscription1).unwrap();
    dispatcher.poll(&mut subscription2).unwrap();

    // Both should have processed all events
    assert_eq!(handler1.count(), 5);
    assert_eq!(handler2.count(), 5);
}

#[test]
fn test_subscription_maintains_independent_bookmarks() {
    let (store, _temp_dir) = create_test_store();

    // Append events
    for i in 0..10 {
        store
            .append(create_signed_payload(&format!("Msg {}", i), i))
            .unwrap();
    }

    let handler1 = CollectorHandler::new("handler1");
    let mut subscription1 =
        PersistentSubscription::new(Arc::clone(store.db()), handler1.clone(), 1000).unwrap();

    let handler2 = CollectorHandler::new("handler2");
    let subscription2 =
        PersistentSubscription::new(Arc::clone(store.db()), handler2.clone(), 1000).unwrap();

    let dispatcher = Dispatcher::new(&store);

    // Poll only first subscription
    dispatcher.poll(&mut subscription1).unwrap();

    assert_eq!(handler1.count(), 10);
    assert_eq!(handler2.count(), 0);

    let bookmark1 = subscription1.bookmark().unwrap();
    let bookmark2 = subscription2.bookmark().unwrap();

    assert_eq!(bookmark1, 9); // Processed up to offset 9
    assert_eq!(bookmark2, 0); // Not processed yet
}

// Helper functions

fn create_test_store() -> (RocksDbEventStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let validator = create_test_validator();
    let config = EventStoreConfig::default();

    let store = RocksDbEventStore::new(
        temp_dir.path().join("events"),
        "test_stream".to_string(),
        "test_space".to_string(),
        validator,
        config,
    )
    .unwrap();

    (store, temp_dir)
}

fn create_signed_payload(message: &str, count: i32) -> SignedPayload<TestPayload> {
    SignedPayload {
        payload: TestPayload {
            message: message.to_string(),
            count,
        },
        signer_did: test_did("alice"),
        signature: fake_signature(message),
        signature_type: "Ed25519".to_string(),
    }
}
