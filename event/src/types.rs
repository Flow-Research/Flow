use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventType(String);

impl EventType {
    pub fn new(event_type: impl Into<String>) -> Self {
        EventType(event_type.into())
    }

    pub fn of(event_type: &'static str) -> Self {
        EventType(event_type.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for EventType {
    fn from(s: &str) -> Self {
        EventType::new(s)
    }
}

impl From<String> for EventType {
    fn from(s: String) -> Self {
        EventType(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_type: EventType,
    pub source_id: String,
    pub timestamp: DateTime<Utc>,
    pub properties: HashMap<String, Value>,
}

impl Event {
    pub fn new(event_type: EventType, source_id: String) -> Self {
        Self {
            event_type,
            source_id,
            timestamp: Utc::now(),
            properties: HashMap::new(),
        }
    }

    pub fn new_at(event_type: EventType, source_id: String, timestamp: DateTime<Utc>) -> Self {
        Self {
            event_type,
            source_id,
            timestamp,
            properties: HashMap::new(),
        }
    }

    pub fn from_type(event_type: impl Into<String>, source_id: String) -> Self {
        Self::new(EventType::new(event_type), source_id)
    }

    // ========== CHAINABLE PROPERTY METHODS ==========

    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    pub fn with_str(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties
            .insert(key.into(), Value::String(value.into()));
        self
    }

    pub fn with_number<T>(mut self, key: impl Into<String>, value: T) -> Self
    where
        T: Into<serde_json::Number>,
    {
        self.properties
            .insert(key.into(), Value::Number(value.into()));
        self
    }

    pub fn with_int(mut self, key: impl Into<String>, value: i64) -> Self {
        self.properties
            .insert(key.into(), Value::Number(value.into()));
        self
    }

    pub fn with_float(mut self, key: impl Into<String>, value: f64) -> Self {
        if let Some(num) = serde_json::Number::from_f64(value) {
            self.properties.insert(key.into(), Value::Number(num));
        }
        self
    }

    pub fn with_bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.properties.insert(key.into(), Value::Bool(value));
        self
    }

    pub fn with_object<T>(mut self, key: impl Into<String>, value: &T) -> Self
    where
        T: Serialize,
    {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.properties.insert(key.into(), json_value);
        }
        self
    }

    pub fn with_array<T>(mut self, key: impl Into<String>, values: Vec<T>) -> Self
    where
        T: Into<Value>,
    {
        let array: Vec<Value> = values.into_iter().map(|v| v.into()).collect();
        self.properties.insert(key.into(), Value::Array(array));
        self
    }

    pub fn with_properties(mut self, properties: HashMap<String, Value>) -> Self {
        self.properties.extend(properties);
        self
    }

    pub fn with_properties_iter<K, V, I>(mut self, properties: I) -> Self
    where
        K: Into<String>,
        V: Into<Value>,
        I: IntoIterator<Item = (K, V)>,
    {
        for (key, value) in properties {
            self.properties.insert(key.into(), value.into());
        }
        self
    }

    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    // ========== GETTER METHODS ==========

    pub fn get_property(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.properties.get(key)?.as_str()
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.properties.get(key)?.as_i64()
    }

    pub fn get_float(&self, key: &str) -> Option<f64> {
        self.properties.get(key)?.as_f64()
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.properties.get(key)?.as_bool()
    }

    pub fn get_array(&self, key: &str) -> Option<&Vec<Value>> {
        self.properties.get(key)?.as_array()
    }

    pub fn get_object<T>(&self, key: &str) -> Result<Option<T>, serde_json::Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        match self.properties.get(key) {
            Some(value) => Ok(Some(serde_json::from_value(value.clone())?)),
            None => Ok(None),
        }
    }

    pub fn has_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    pub fn property_count(&self) -> usize {
        self.properties.len()
    }

    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    // ========== UTILITY METHODS ==========

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Event[{}] from {} at {} ({} properties)",
            self.event_type,
            self.source_id,
            self.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            self.properties.len()
        )
    }
}
