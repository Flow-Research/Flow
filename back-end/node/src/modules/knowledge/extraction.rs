use serde::{Deserialize, Serialize};
use swiftide::integrations;
use thiserror::Error;
use tracing::{debug, warn};

use super::types::EntityType;

#[derive(Debug, Error)]
pub enum ExtractionError {
    #[error("LLM error: {0}")]
    LlmError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("No entities found")]
    NoEntities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub entity_type: EntityType,
    pub name: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmEntityResponse {
    #[serde(rename = "type")]
    entity_type: String,
    name: String,
    confidence: f32,
}

pub struct EntityExtractor {
    ollama: integrations::ollama::Ollama,
    confidence_threshold: f32,
}

impl EntityExtractor {
    pub fn new() -> Self {
        Self {
            ollama: integrations::ollama::Ollama::default()
                .with_default_prompt_model("llama3.1:8b")
                .to_owned(),
            confidence_threshold: 0.7,
        }
    }

    pub fn with_model(model: &str) -> Self {
        Self {
            ollama: integrations::ollama::Ollama::default()
                .with_default_prompt_model(model)
                .to_owned(),
            confidence_threshold: 0.7,
        }
    }

    pub fn with_confidence_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    pub async fn extract(&self, text: &str) -> Result<Vec<ExtractedEntity>, ExtractionError> {
        let prompt = build_extraction_prompt(text);

        let response = self
            .call_llm(&prompt)
            .await
            .map_err(|e| ExtractionError::LlmError(e.to_string()))?;

        let entities = parse_extraction_response(&response)?;

        let filtered: Vec<ExtractedEntity> = entities
            .into_iter()
            .filter(|e| e.confidence >= self.confidence_threshold)
            .collect();

        if filtered.is_empty() {
            debug!("No entities passed confidence threshold");
        }

        Ok(filtered)
    }

    async fn call_llm(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use swiftide::traits::SimplePrompt;

        let response = self.ollama.prompt(prompt.to_string().into()).await?;
        Ok(response)
    }
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn build_extraction_prompt(text: &str) -> String {
    format!(
        r#"Extract entities from the following text. Return a JSON array.

Entity types: Person, Concept, Date, Location
For each entity, provide: type, name, confidence (0-1)

Text:
{}

Response format (ONLY output valid JSON, no markdown):
[
  {{"type": "Person", "name": "John Smith", "confidence": 0.9}},
  {{"type": "Concept", "name": "machine learning", "confidence": 0.8}}
]

If no entities are found, return an empty array: []

Entities:"#,
        truncate_text(text, 4000)
    )
}

fn truncate_text(text: &str, max_chars: usize) -> &str {
    if text.len() <= max_chars {
        text
    } else {
        &text[..max_chars]
    }
}

fn parse_extraction_response(response: &str) -> Result<Vec<ExtractedEntity>, ExtractionError> {
    let cleaned = clean_json_response(response);

    let parsed: Vec<LlmEntityResponse> = serde_json::from_str(&cleaned).map_err(|e| {
        warn!("Failed to parse LLM response: {}", e);
        debug!("Response was: {}", response);
        ExtractionError::ParseError(format!("Invalid JSON: {}", e))
    })?;

    let entities: Vec<ExtractedEntity> = parsed
        .into_iter()
        .filter_map(|e| {
            let entity_type = match e.entity_type.to_lowercase().as_str() {
                "person" => Some(EntityType::Person),
                "concept" => Some(EntityType::Concept),
                "date" => Some(EntityType::Date),
                "location" => Some(EntityType::Location),
                "document" => Some(EntityType::Document),
                _ => {
                    debug!("Unknown entity type: {}", e.entity_type);
                    None
                }
            };

            entity_type.map(|et| ExtractedEntity {
                entity_type: et,
                name: e.name.trim().to_string(),
                confidence: e.confidence.clamp(0.0, 1.0),
            })
        })
        .filter(|e| !e.name.is_empty())
        .collect();

    Ok(entities)
}

fn clean_json_response(response: &str) -> String {
    let response = response.trim();

    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            return response[start..=end].to_string();
        }
    }

    if response.starts_with("```json") {
        let content = response.strip_prefix("```json").unwrap_or(response);
        let content = content.strip_suffix("```").unwrap_or(content);
        return content.trim().to_string();
    }

    if response.starts_with("```") {
        let content = response.strip_prefix("```").unwrap_or(response);
        let content = content.strip_suffix("```").unwrap_or(content);
        return content.trim().to_string();
    }

    response.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_response() {
        let response = r#"[
            {"type": "Person", "name": "John Doe", "confidence": 0.9},
            {"type": "Concept", "name": "machine learning", "confidence": 0.85}
        ]"#;

        let entities = parse_extraction_response(response).unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].name, "John Doe");
        assert_eq!(entities[0].entity_type, EntityType::Person);
        assert_eq!(entities[1].name, "machine learning");
        assert_eq!(entities[1].entity_type, EntityType::Concept);
    }

    #[test]
    fn test_parse_markdown_wrapped_response() {
        let response = r#"```json
[
    {"type": "Location", "name": "New York", "confidence": 0.95}
]
```"#;

        let entities = parse_extraction_response(response).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "New York");
        assert_eq!(entities[0].entity_type, EntityType::Location);
    }

    #[test]
    fn test_parse_with_preamble() {
        let response = r#"Here are the entities I found:

[{"type": "Person", "name": "Alice", "confidence": 0.8}]

I hope this helps!"#;

        let entities = parse_extraction_response(response).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "Alice");
    }

    #[test]
    fn test_parse_empty_array() {
        let response = "[]";
        let entities = parse_extraction_response(response).unwrap();
        assert!(entities.is_empty());
    }

    #[test]
    fn test_parse_unknown_type() {
        let response = r#"[
            {"type": "Organization", "name": "Acme Corp", "confidence": 0.9},
            {"type": "Person", "name": "Bob", "confidence": 0.8}
        ]"#;

        let entities = parse_extraction_response(response).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "Bob");
    }

    #[test]
    fn test_confidence_clamping() {
        let response = r#"[{"type": "Person", "name": "Test", "confidence": 1.5}]"#;
        let entities = parse_extraction_response(response).unwrap();
        assert_eq!(entities[0].confidence, 1.0);
    }

    #[test]
    fn test_build_prompt() {
        let prompt = build_extraction_prompt("This is a test about John.");
        assert!(prompt.contains("This is a test about John."));
        assert!(prompt.contains("Person"));
        assert!(prompt.contains("Concept"));
    }
}
