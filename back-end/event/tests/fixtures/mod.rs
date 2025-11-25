#![allow(dead_code)]

use event::{EventPayload, EventValidator};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Simple test payload for basic tests
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TestPayload {
    pub message: String,
    pub count: i32,
}

impl EventPayload for TestPayload {
    const TYPE: &'static str = "TestEvent";
    const VERSION: u32 = 1;
}

/// Complex nested payload for testing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ComplexPayload {
    pub nested: NestedData,
    pub list: Vec<String>,
    pub optional: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NestedData {
    pub field1: String,
    pub field2: i32,
}

impl EventPayload for ComplexPayload {
    const TYPE: &'static str = "ComplexEvent";
    const VERSION: u32 = 1;
}

/// Multiple versions for migration testing
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PayloadV1 {
    pub name: String,
}

impl EventPayload for PayloadV1 {
    const TYPE: &'static str = "VersionedEvent";
    const VERSION: u32 = 1;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PayloadV2 {
    pub name: String,
    pub description: String, // Added in v2
}

impl EventPayload for PayloadV2 {
    const TYPE: &'static str = "VersionedEvent";
    const VERSION: u32 = 2;
}

/// Creates a test validator with common schemas registered
pub fn create_test_validator() -> EventValidator {
    let validator = EventValidator::new();

    // Register TestEvent schema
    let test_schema = json!({
        "title": "TestEvent",
        "version": 1,
        "type": "object",
        "properties": {
            "message": { "type": "string" },
            "count": { "type": "integer" }
        },
        "required": ["message", "count"]
    });
    validator.register_schema(test_schema).unwrap();

    // Register ComplexEvent schema
    let complex_schema = json!({
        "title": "ComplexEvent",
        "version": 1,
        "type": "object",
        "properties": {
            "nested": {
                "type": "object",
                "properties": {
                    "field1": { "type": "string" },
                    "field2": { "type": "integer" }
                },
                "required": ["field1", "field2"]
            },
            "list": {
                "type": "array",
                "items": { "type": "string" }
            },
            "optional": { "type": ["integer", "null"] }
        },
        "required": ["nested", "list"]
    });
    validator.register_schema(complex_schema).unwrap();

    // Register versioned schemas
    let v1_schema = json!({
        "title": "VersionedEvent",
        "version": 1,
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "required": ["name"]
    });
    validator.register_schema(v1_schema).unwrap();

    let v2_schema = json!({
        "title": "VersionedEvent",
        "version": 2,
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "description": { "type": "string" }
        },
        "required": ["name", "description"]
    });
    validator.register_schema(v2_schema).unwrap();

    validator
}

/// Helper to create test DIDs
pub fn test_did(name: &str) -> String {
    format!("did:test:{}", name)
}

/// Helper to create fake but valid-looking signatures
pub fn fake_signature(data: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data.as_bytes());
    format!("{:x}", hash)
}
