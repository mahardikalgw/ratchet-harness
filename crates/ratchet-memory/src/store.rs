use crate::MemoryResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Persistent project memory store.
pub struct ProjectMemory {
    path: PathBuf,
    entries: HashMap<String, MemoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub kind: MemoryKind,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub importance: u8, // 1-10
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryKind {
    ArchitectureDecision,
    Gotcha,
    VerificationReport,
    ApiPattern,
    BugFix,
    RefactorNote,
}

impl ProjectMemory {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: HashMap::new(),
        }
    }

    pub async fn load(&mut self) -> MemoryResult<()> {
        if self.path.exists() {
            let content = tokio::fs::read_to_string(&self.path).await?;
            let entries: Vec<MemoryEntry> = serde_json::from_str(&content)?;
            self.entries = entries.into_iter().map(|e| (e.id.clone(), e)).collect();
        }
        Ok(())
    }

    pub async fn save(&self) -> MemoryResult<()> {
        let entries: Vec<_> = self.entries.values().cloned().collect();
        let content = serde_json::to_string_pretty(&entries)?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, content).await?;
        Ok(())
    }

    pub fn add(&mut self, entry: MemoryEntry) {
        self.entries.insert(entry.id.clone(), entry);
    }

    pub fn get(&self, id: &str) -> Option<&MemoryEntry> {
        self.entries.get(id)
    }

    pub fn query(&self, tag: &str) -> Vec<&MemoryEntry> {
        self.entries
            .values()
            .filter(|e| e.tags.iter().any(|t| t == tag))
            .collect()
    }

    pub fn summarize(&self, max_entries: usize) -> String {
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.importance));
        entries.truncate(max_entries);

        entries
            .iter()
            .map(|e| {
                format!(
                    "- [{}] {}: {}",
                    e.kind_string(),
                    e.id,
                    e.content.lines().next().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn compact(&mut self) {
        // Remove low-importance entries if we have too many
        if self.entries.len() > 1000 {
            let mut entries: Vec<_> = self.entries.values().cloned().collect();
            entries.sort_by_key(|e| e.importance);
            let to_remove = entries.len() - 500;
            for entry in entries.into_iter().take(to_remove) {
                self.entries.remove(&entry.id);
            }
        }
    }
}

impl MemoryEntry {
    pub fn kind_string(&self) -> &'static str {
        match self.kind {
            MemoryKind::ArchitectureDecision => "arch",
            MemoryKind::Gotcha => "gotcha",
            MemoryKind::VerificationReport => "verify",
            MemoryKind::ApiPattern => "pattern",
            MemoryKind::BugFix => "bugfix",
            MemoryKind::RefactorNote => "refactor",
        }
    }
}

/// A simple in-memory store for working context.
pub struct MemoryStore {
    chunks: Vec<String>,
    _max_tokens: usize,
}

impl MemoryStore {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            chunks: Vec::new(),
            _max_tokens: max_tokens,
        }
    }

    pub fn add(&mut self, chunk: String) {
        self.chunks.push(chunk);
    }

    pub fn retrieve(&self, _query: &str) -> Vec<&String> {
        // Simple implementation: return all chunks
        // In production, would use embeddings/vector search
        self.chunks.iter().collect()
    }
}
