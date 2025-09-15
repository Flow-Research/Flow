use crate::{
    errors::StorageError,
    types::{Query, QueryTarget, Response},
};

pub trait Store {
    fn process(&self, query: Query) -> Result<Response, StorageError>;

    fn target(&self) -> QueryTarget;
}

pub struct StoreManager {
    stores: Vec<Box<dyn Store>>,
}

impl StoreManager {
    pub fn new(stores: Vec<Box<dyn Store>>) -> Self {
        StoreManager { stores }
    }

    pub fn process(&self, query: Query) -> Result<Response, StorageError> {
        let query_target = &query.target;

        let store = self
            .get_store(query_target)
            .ok_or_else(|| StorageError::StoreNotFound {
                target: *query_target,
            })?;

        println!("Dispatching query to {:?} store.", store.target());
        store.process(query)
    }

    pub fn get_store(&self, target: &QueryTarget) -> Option<&dyn Store> {
        self.stores
            .iter()
            .find(|s| s.target() == *target)
            .map(|s| s.as_ref())
    }
}
