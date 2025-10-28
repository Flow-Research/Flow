use super::types::ContentType;
use once_cell::sync::Lazy;
use regex::Regex;

// Compile regexes once at startup
static CODE_PATTERNS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(fn |function |class |def |import |const |let |var |=>|->|\{|\}|;$)").unwrap()
});

static MARKDOWN_PATTERNS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)(^#{1,6}\s|```|^\*\*|^\-\s|\[.*\]\(.*\))").unwrap());

static SCIENTIFIC_PATTERNS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(abstract|introduction|methodology|results|conclusion|references|et al\.|fig\.|table)",
    )
    .unwrap()
});

/// Content type detector
pub struct ContentDetector;

impl ContentDetector {
    /// Detect content type from text using heuristics
    pub fn detect(text: &str) -> ContentType {
        if text.is_empty() {
            return ContentType::PlainText;
        }

        // Sample first 3000 chars for performance
        let sample = Self::get_sample(text, 3000);

        let code_score = Self::score_code(&sample);
        let markdown_score = Self::score_markdown(text); // Use full text for headers
        let scientific_score = Self::score_scientific(&sample);

        tracing::debug!(
            code_score,
            markdown_score,
            scientific_score,
            "Content type detection scores"
        );

        // Decision tree - order matters
        if markdown_score >= 3 {
            ContentType::Markdown
        } else if code_score >= 4 {
            ContentType::Code
        } else if scientific_score >= 3 {
            ContentType::Scientific
        } else {
            ContentType::PlainText
        }
    }

    /// Get a sample of the text for analysis
    fn get_sample(text: &str, max_chars: usize) -> String {
        if text.len() <= max_chars {
            text.to_string()
        } else {
            // Take first portion for analysis
            text.chars().take(max_chars).collect()
        }
    }

    /// Score likelihood of code content
    fn score_code(sample: &str) -> usize {
        CODE_PATTERNS.find_iter(sample).count()
    }

    /// Score likelihood of markdown content
    fn score_markdown(text: &str) -> usize {
        MARKDOWN_PATTERNS.find_iter(text).count()
    }

    /// Score likelihood of scientific content
    fn score_scientific(sample: &str) -> usize {
        SCIENTIFIC_PATTERNS.find_iter(sample).count()
    }

    /// Detect language using whatlang
    pub fn detect_language(text: &str) -> Option<String> {
        whatlang::detect(text).map(|info| info.lang().code().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_empty() {
        assert_eq!(ContentDetector::detect(""), ContentType::PlainText);
    }

    #[test]
    fn test_detect_code() {
        let code = r#"
            fn main() {
                let x = 5;
                println!("Hello");
            }
        "#;
        assert_eq!(ContentDetector::detect(code), ContentType::Code);
    }

    #[test]
    fn test_detect_markdown() {
        let markdown = r#"
# Title

Some content

## Subtitle

```rust
fn test() {}
```

- Item 1
- Item 2
        "#;
        assert_eq!(ContentDetector::detect(markdown), ContentType::Markdown);
    }

    #[test]
    fn test_detect_scientific() {
        let paper = r#"
Abstract: This paper presents a novel approach to...

Introduction

The methodology employed in this study...

Results

As shown in Fig. 1, the data indicates...

Conclusion

References
Smith et al. (2020)
        "#;
        assert_eq!(ContentDetector::detect(paper), ContentType::Scientific);
    }

    #[test]
    fn test_detect_plain_text() {
        let plain = "This is just some plain text. Nothing special here. Just regular sentences.";
        assert_eq!(ContentDetector::detect(plain), ContentType::PlainText);
    }

    #[test]
    fn test_detect_language() {
        let english = "This is an English sentence.";
        let lang = ContentDetector::detect_language(english);
        assert_eq!(lang, Some("eng".to_string()));

        let spanish = "Esta es una oración en español.";
        let lang = ContentDetector::detect_language(spanish);
        assert_eq!(lang, Some("spa".to_string()));
    }
}
