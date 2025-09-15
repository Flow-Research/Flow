use std::{
    error::Error,
    sync::{Arc, RwLock},
};

use crate::types::Event;

pub trait EventListener: Send + Sync {
    fn handle(&self, event: &Event) -> Result<(), Box<dyn Error>>;
}

#[derive(Clone)]
pub struct EventListenerManager {
    listeners: Arc<RwLock<Vec<Arc<dyn EventListener>>>>,
}

impl EventListenerManager {
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn subscribe(&mut self, listener: Arc<dyn EventListener>) {
        let mut listeners = self.listeners.write().unwrap();
        listeners.push(listener);
    }

    pub fn publish(&self, event: &Event) -> Result<(), Box<dyn Error>> {
        let listeners = self.listeners.read().unwrap();
        for listener in listeners.iter() {
            listener.handle(event)?;
        }
        Ok(())
    }

    pub fn listener_count(&self) -> usize {
        self.listeners.read().unwrap().len()
    }

    pub fn unsubscribe_all(&mut self) {
        let mut listeners = self.listeners.write().unwrap();
        listeners.clear();
    }
}

impl Default for EventListenerManager {
    fn default() -> Self {
        Self::new()
    }
}

pub trait EventSource {
    fn event_manager(&self) -> &EventListenerManager;

    fn event_manager_mut(&mut self) -> &mut EventListenerManager;

    fn subscribe(&mut self, listener: Arc<dyn EventListener>) {
        self.event_manager_mut().subscribe(listener);
    }

    fn publish(&self, event: &Event) -> Result<(), Box<dyn Error>> {
        self.event_manager().publish(event)
    }

    fn unsubscribe_all(&mut self) {
        self.event_manager_mut().unsubscribe_all();
    }

    fn listener_count(&self) -> usize {
        self.event_manager().listener_count()
    }
}
