use crate::utils::{ensure_dir, today_date};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MemoryStore {
    pub workspace: PathBuf,
    pub memory_dir: PathBuf,
    pub memory_file: PathBuf,
}

impl MemoryStore {
    pub fn new(workspace: PathBuf) -> std::io::Result<Self> {
        let memory_dir = ensure_dir(&workspace.join("memory"))?;
        let memory_file = memory_dir.join("MEMORY.md");
        Ok(Self {
            workspace,
            memory_dir,
            memory_file,
        })
    }

    pub fn get_today_file(&self) -> PathBuf {
        self.memory_dir.join(format!("{}.md", today_date()))
    }

    pub fn read_today(&self) -> String {
        let path = self.get_today_file();
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub fn append_today(&self, content: &str) -> std::io::Result<()> {
        let path = self.get_today_file();
        if path.exists() {
            let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
            if !existing.is_empty() {
                existing.push('\n');
            }
            existing.push_str(content);
            std::fs::write(path, existing)?;
        } else {
            let body = format!("# {}\n\n{}", today_date(), content);
            std::fs::write(path, body)?;
        }
        Ok(())
    }

    pub fn read_long_term(&self) -> String {
        std::fs::read_to_string(&self.memory_file).unwrap_or_default()
    }

    pub fn write_long_term(&self, content: &str) -> std::io::Result<()> {
        std::fs::write(&self.memory_file, content)
    }

    pub fn get_memory_context(&self) -> String {
        let mut parts = Vec::new();
        let long_term = self.read_long_term();
        if !long_term.is_empty() {
            parts.push(format!("## Long-term Memory\n{}", long_term));
        }
        let today = self.read_today();
        if !today.is_empty() {
            parts.push(format!("## Today's Notes\n{}", today));
        }
        parts.join("\n\n")
    }
}
