use crate::{
    format::{SpecFile, SpecFrontmatter, SpecSection},
    SpecError, SpecResult,
};
use indexmap::IndexMap;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use regex::Regex;

/// Parses `.spec.md` files into structured `SpecFile` objects.
pub struct SpecParser;

impl SpecParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, content: &str) -> SpecResult<SpecFile> {
        let (frontmatter_str, body) = Self::extract_frontmatter(content)?;
        let frontmatter: SpecFrontmatter = if frontmatter_str.trim().is_empty() {
            SpecFrontmatter::default()
        } else {
            serde_yaml::from_str(frontmatter_str)
                .map_err(|e| SpecError::Parse(format!("invalid frontmatter: {e}")))?
        };

        let sections = Self::parse_sections(body)?;

        Ok(SpecFile {
            frontmatter,
            sections,
            raw: content.to_string(),
        })
    }

    pub fn parse_file(&self, path: &std::path::Path) -> SpecResult<SpecFile> {
        let content =
            std::fs::read_to_string(path).map_err(|e| SpecError::Io(e.to_string()))?;
        self.parse(&content)
    }

    fn extract_frontmatter(content: &str) -> SpecResult<(&str, &str)> {
        let re = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)$").unwrap();
        if let Some(caps) = re.captures(content) {
            let fm = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let body = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            Ok((fm, body))
        } else {
            // No frontmatter — entire content is body
            Ok(("", content))
        }
    }

    fn parse_sections(body: &str) -> SpecResult<Vec<SpecSection>> {
        let parser = Parser::new(body);
        let mut sections: Vec<SpecSection> = Vec::new();
        let mut current_heading: Option<String> = None;
        let mut current_level: u8 = 0;
        let mut current_body = String::new();
        let mut in_heading = false;

        for event in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    // Flush the section that just ended.
                    let body = current_body.trim().to_string();
                    if current_heading.is_some() || !body.is_empty() {
                        sections.push(SpecSection {
                            heading: current_heading.take(),
                            level: current_level,
                            body,
                            metadata: IndexMap::new(),
                        });
                    }
                    current_body.clear();
                    current_level = level as u8;
                    in_heading = true;
                }
                Event::End(TagEnd::Heading(_)) => {
                    in_heading = false;
                }
                // Reconstruct bullet markers so list items stay line-separated
                // and remain parseable by the extractor.
                Event::Start(Tag::Item) => {
                    if !current_body.is_empty() && !current_body.ends_with('\n') {
                        current_body.push('\n');
                    }
                    current_body.push_str("- ");
                }
                Event::End(TagEnd::Item)
                    if !current_body.ends_with('\n') => {
                        current_body.push('\n');
                    }
                Event::Start(Tag::Paragraph)
                    if !current_body.is_empty() && !current_body.ends_with('\n') => {
                        current_body.push('\n');
                    }
                Event::End(TagEnd::Paragraph)
                    if !current_body.ends_with('\n') => {
                        current_body.push('\n');
                    }
                Event::Start(Tag::CodeBlock(_)) => {
                    if !current_body.is_empty() && !current_body.ends_with('\n') {
                        current_body.push('\n');
                    }
                    current_body.push_str("```\n");
                }
                Event::End(TagEnd::CodeBlock) => {
                    if !current_body.ends_with('\n') {
                        current_body.push('\n');
                    }
                    current_body.push_str("```\n");
                }
                Event::Text(text) => {
                    if in_heading {
                        current_heading = Some(text.to_string());
                    } else {
                        current_body.push_str(&text);
                    }
                }
                Event::Code(code) => {
                    current_body.push('`');
                    current_body.push_str(&code);
                    current_body.push('`');
                }
                Event::SoftBreak | Event::HardBreak => {
                    current_body.push('\n');
                }
                Event::Html(html) => {
                    current_body.push_str(&html);
                }
                _ => {}
            }
        }

        // Flush the trailing section.
        let body = current_body.trim().to_string();
        if current_heading.is_some() || !body.is_empty() {
            sections.push(SpecSection {
                heading: current_heading.take(),
                level: current_level,
                body,
                metadata: IndexMap::new(),
            });
        } else if sections.is_empty() {
            sections.push(SpecSection {
                heading: None,
                level: 0,
                body: body.trim().to_string(),
                metadata: IndexMap::new(),
            });
        }

        Ok(sections)
    }
}

impl Default for SpecParser {
    fn default() -> Self {
        Self::new()
    }
}
