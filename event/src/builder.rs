use std::sync::Arc;

use crate::source::{
    EventSource,
    EventListener, 
    EventListenerManager
};


pub struct EventSourceBuilder<T> {
    inner: T,
    event_manager: EventListenerManager,
}


impl<T> EventSourceBuilder<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            event_manager: EventListenerManager::new(),
        }
    }

    pub fn add_listener(mut self, listener: Box<dyn EventListener>) -> Self {
        self.event_manager.subscribe(Arc::from(listener));
        self
    }

    pub fn build(self) -> EventSourceWrapper<T> {
        EventSourceWrapper {
            inner: self.inner,
            event_manager: self.event_manager,
        }
    }
}


pub struct EventSourceWrapper<T> {
    pub inner: T,
    event_manager: EventListenerManager,
}


impl<T> EventSource for EventSourceWrapper<T> {

    fn event_manager(&self) -> &EventListenerManager {
        &self.event_manager
    }

    fn event_manager_mut(&mut self) -> &mut EventListenerManager {
        &mut self.event_manager
    }

}


