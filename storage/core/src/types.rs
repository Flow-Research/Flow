use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum QueryTarget {
    KV,
    SQLITE,
    GRAPH,
}

pub struct Query {
    pub target: QueryTarget,
    pub map: HashMap<String, String>,
}

impl Query {
    pub fn from_target(target: QueryTarget) -> Query {
        Query {
            target,
            map: HashMap::new(),
        }
    }

    pub fn from(target: QueryTarget, map: HashMap<String, String>) -> Query {
        Query { target, map }
    }

    pub fn insert(&mut self, key: &str, value: &str) -> &mut Query {
        self.map.insert(key.to_string(), value.to_string());
        self
    }
}

/// Response encapsulates a response from the storage layer
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Response {
    data: Value,
}
