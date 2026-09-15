use crate::{
    SpecError, SpecResult,
    format::{SpecFile, SpecFrontmatter, SpecSection},
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
        let content = std::fs::read_to_string(path).map_err(|e| SpecError::Io(e.to_string()))?;
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

    /// Split the document into sections, preserving the raw source text.
    ///
    /// Earlier versions rebuilt the body from markdown events, which silently
    /// mangled anything markdown treats as syntax: `app/__init__.py` became
    /// `app/init.py` because `__init__` parsed as bold. Slicing the original
    /// document by heading offsets keeps every character exactly as written.
    fn parse_sections(body: &str) -> SpecResult<Vec<SpecSection>> {
        let mut sections: Vec<SpecSection> = Vec::new();
        let mut current: Option<(u8, String)> = None;
        // Where the current section's content begins in `body`.
        let mut content_start = 0usize;
        let mut in_heading = false;
        let mut heading_text = String::new();

        for (event, range) in Parser::new(body).into_offset_iter() {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    // Everything before this heading belongs to the previous one.
                    let slice = body[content_start..range.start].trim().to_string();
                    match current.take() {
                        Some((level, title)) => sections.push(SpecSection {
                            heading: Some(title),
                            level,
                            body: slice,
                            metadata: IndexMap::new(),
                        }),
                        None if !slice.is_empty() => sections.push(SpecSection {
                            heading: None,
                            level: 0,
                            body: slice,
                            metadata: IndexMap::new(),
                        }),
                        None => {}
                    }

                    heading_text.clear();
                    in_heading = true;
                    current = Some((level as u8, String::new()));
                }

                Event::Text(text) if in_heading => heading_text.push_str(&text),

                Event::End(TagEnd::Heading(_)) => {
                    in_heading = false;
                    if let Some((level, _)) = current {
                        current = Some((level, heading_text.clone()));
                    }
                    // Content starts after the rest of the heading's line.
                    content_start = body[range.end..]
                        .find('\n')
                        .map(|i| range.end + i + 1)
                        .unwrap_or(body.len());
                }

                _ => {}
            }
        }

        // Trailing section.
        let slice = body[content_start..].trim().to_string();
        match current.take() {
            Some((level, title)) => sections.push(SpecSection {
                heading: Some(title),
                level,
                body: slice,
                metadata: IndexMap::new(),
            }),
            None => {
                if !slice.is_empty() || sections.is_empty() {
                    sections.push(SpecSection {
                        heading: None,
                        level: 0,
                        body: slice,
                        metadata: IndexMap::new(),
                    });
                }
            }
        }

        Ok(sections)
    }
}

impl Default for SpecParser {
    fn default() -> Self {
        Self::new()
    }
}
