use crate::{MemoryResult, ProjectMemory};
use ratchet_spec::{SpecFile, TaskId};

/// Assembles working context for a task from specs, memory, and code slices.
pub struct ContextAssembler;

impl ContextAssembler {
    pub fn new() -> Self {
        Self
    }

    pub fn assemble(
        &self,
        spec: &SpecFile,
        task_id: &TaskId,
        memory: &ProjectMemory,
        relevant_files: &[String],
    ) -> MemoryResult<WorkingContext> {
        let mut sections = Vec::new();

        // Spec context
        sections.push(ContextSection {
            title: "Spec".to_string(),
            content: format!("# {}\n\n{}", spec.frontmatter.title, spec.raw),
            priority: 10,
        });

        // Task context
        sections.push(ContextSection {
            title: "Task".to_string(),
            content: format!("Current task: {}", task_id.0),
            priority: 9,
        });

        // Memory summary
        let memory_summary = memory.summarize(10);
        if !memory_summary.is_empty() {
            sections.push(ContextSection {
                title: "Project Memory".to_string(),
                content: memory_summary,
                priority: 5,
            });
        }

        // Relevant files
        for file in relevant_files {
            sections.push(ContextSection {
                title: format!("File: {}", file),
                content: "(content loaded on demand)".to_string(),
                priority: 3,
            });
        }

        // Sort by priority and assemble
        sections.sort_by_key(|s| std::cmp::Reverse(s.priority));

        let assembled = sections
            .iter()
            .map(|s| format!("## {}\n{}", s.title, s.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(WorkingContext {
            content: assembled,
            sections,
        })
    }
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkingContext {
    pub content: String,
    pub sections: Vec<ContextSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextSection {
    pub title: String,
    pub content: String,
    pub priority: u8,
}
