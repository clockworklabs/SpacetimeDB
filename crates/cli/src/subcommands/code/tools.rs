//! File operation tools for AI-driven code changes.

use crate::subcommands::code::state::{PendingChange, PendingChangeStatus, PendingChangeType};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};

/// A tool definition for the AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call from the AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// The result of executing a tool.
#[derive(Debug, Clone)]
pub enum ToolResult {
    /// The tool executed successfully with output.
    Success(String),
    /// The tool requires user approval before executing (e.g., file writes).
    NeedsApproval(PendingChange),
    /// The tool failed.
    Error(String),
}

/// File tools executor.
pub struct FileTools {
    project_dir: PathBuf,
    backup_dir: PathBuf,
}

impl FileTools {
    /// Create a new file tools executor.
    pub fn new(project_dir: PathBuf) -> Result<Self> {
        let backup_dir = project_dir.join(".spacetime").join("backups");
        fs::create_dir_all(&backup_dir).context("Failed to create backup directory")?;

        Ok(Self {
            project_dir,
            backup_dir,
        })
    }

    /// Execute a tool call.
    pub fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        match tool_call.name.as_str() {
            "read_file" => self.read_file(tool_call),
            "write_file" => self.write_file(tool_call),
            "edit_file" => self.edit_file(tool_call),
            "list_files" => self.list_files(tool_call),
            _ => ToolResult::Error(format!("Unknown tool: {}", tool_call.name)),
        }
    }

    /// Read a file's contents.
    fn read_file(&self, tool_call: &ToolCall) -> ToolResult {
        let path = match tool_call.input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::Error("Missing 'path' parameter".to_string()),
        };

        let full_path = self.resolve_path(path);
        if !self.is_safe_path(&full_path) {
            return ToolResult::Error("Path is outside project directory".to_string());
        }

        match fs::read_to_string(&full_path) {
            Ok(content) => ToolResult::Success(content),
            Err(e) => ToolResult::Error(format!("Failed to read file: {}", e)),
        }
    }

    /// Write a file (requires approval).
    fn write_file(&self, tool_call: &ToolCall) -> ToolResult {
        let path = match tool_call.input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::Error("Missing 'path' parameter".to_string()),
        };

        let content = match tool_call.input.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::Error("Missing 'content' parameter".to_string()),
        };

        let full_path = self.resolve_path(path);
        if !self.is_safe_path(&full_path) {
            return ToolResult::Error("Path is outside project directory".to_string());
        }

        // Check if file exists to determine if this is create or overwrite
        let (change_type, diff) = if full_path.exists() {
            let old_content = match fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(e) => return ToolResult::Error(format!("Failed to read existing file: {}", e)),
            };
            let diff = generate_diff(&old_content, content, path);
            (
                PendingChangeType::Edit {
                    old: old_content,
                    new: content.to_string(),
                },
                diff,
            )
        } else {
            let diff = generate_create_diff(content, path);
            (
                PendingChangeType::Create {
                    content: content.to_string(),
                },
                diff,
            )
        };

        ToolResult::NeedsApproval(PendingChange {
            id: tool_call.id.clone(),
            path: full_path,
            change_type,
            diff,
            status: PendingChangeStatus::Pending,
        })
    }

    /// Edit a file (requires approval).
    fn edit_file(&self, tool_call: &ToolCall) -> ToolResult {
        let path = match tool_call.input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::Error("Missing 'path' parameter".to_string()),
        };

        let old = match tool_call.input.get("old").and_then(|v| v.as_str()) {
            Some(o) => o,
            None => return ToolResult::Error("Missing 'old' parameter".to_string()),
        };

        let new = match tool_call.input.get("new").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::Error("Missing 'new' parameter".to_string()),
        };

        let full_path = self.resolve_path(path);
        if !self.is_safe_path(&full_path) {
            return ToolResult::Error("Path is outside project directory".to_string());
        }

        // Read the file and verify the old content exists
        let file_content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => return ToolResult::Error(format!("Failed to read file: {}", e)),
        };

        if !file_content.contains(old) {
            return ToolResult::Error("Could not find the specified text to replace".to_string());
        }

        // Generate the new content
        let new_content = file_content.replacen(old, new, 1);
        let diff = generate_diff(&file_content, &new_content, path);

        ToolResult::NeedsApproval(PendingChange {
            id: tool_call.id.clone(),
            path: full_path,
            change_type: PendingChangeType::Edit {
                old: file_content,
                new: new_content,
            },
            diff,
            status: PendingChangeStatus::Pending,
        })
    }

    /// List files in a directory.
    fn list_files(&self, tool_call: &ToolCall) -> ToolResult {
        let path = match tool_call.input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::Error("Missing 'path' parameter".to_string()),
        };

        let recursive = tool_call
            .input
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let full_path = self.resolve_path(path);
        if !self.is_safe_path(&full_path) {
            return ToolResult::Error("Path is outside project directory".to_string());
        }

        if !full_path.is_dir() {
            return ToolResult::Error("Path is not a directory".to_string());
        }

        let files = if recursive {
            list_files_recursive(&full_path, &self.project_dir)
        } else {
            list_files_flat(&full_path, &self.project_dir)
        };

        match files {
            Ok(list) => ToolResult::Success(list.join("\n")),
            Err(e) => ToolResult::Error(format!("Failed to list files: {}", e)),
        }
    }

    /// Resolve a relative path to an absolute path within the project.
    fn resolve_path(&self, path: &str) -> PathBuf {
        if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.project_dir.join(path)
        }
    }

    /// Check if a path is within the project directory.
    fn is_safe_path(&self, path: &Path) -> bool {
        match (path.canonicalize(), self.project_dir.canonicalize()) {
            (Ok(p), Ok(base)) => p.starts_with(base),
            // If canonicalize fails (file doesn't exist), check the parent
            (Err(_), Ok(base)) => {
                if let Some(parent) = path.parent() {
                    parent.canonicalize().map(|p| p.starts_with(base)).unwrap_or(false)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Apply a pending change.
    pub fn apply_change(&self, change: &PendingChange) -> Result<()> {
        // Create backup if file exists
        if change.path.exists() {
            let backup_name = format!(
                "{}_{}.bak",
                change.path.file_name().unwrap_or_default().to_string_lossy(),
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            );
            let backup_path = self.backup_dir.join(backup_name);
            fs::copy(&change.path, backup_path).context("Failed to create backup")?;
        }

        match &change.change_type {
            PendingChangeType::Create { content } | PendingChangeType::Edit { new: content, .. } => {
                // Ensure parent directory exists
                if let Some(parent) = change.path.parent() {
                    fs::create_dir_all(parent).context("Failed to create parent directory")?;
                }
                fs::write(&change.path, content).context("Failed to write file")?;
            }
            PendingChangeType::Delete => {
                fs::remove_file(&change.path).context("Failed to delete file")?;
            }
        }

        Ok(())
    }
}

/// Generate a unified diff between two strings.
fn generate_diff(old: &str, new: &str, path: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();

    output.push_str(&format!("--- a/{}\n", path));
    output.push_str(&format!("+++ b/{}\n", path));

    for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
        if idx > 0 {
            output.push('\n');
        }

        for op in group {
            for change in diff.iter_inline_changes(op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };

                output.push_str(sign);
                for (_, value) in change.iter_strings_lossy() {
                    output.push_str(&value);
                }
                if change.missing_newline() {
                    output.push('\n');
                }
            }
        }
    }

    output
}

/// Generate a diff for a new file creation.
fn generate_create_diff(content: &str, path: &str) -> String {
    let mut output = String::new();

    output.push_str(&format!("--- /dev/null\n"));
    output.push_str(&format!("+++ b/{}\n", path));

    for line in content.lines() {
        output.push_str(&format!("+{}\n", line));
    }

    output
}

/// List files in a directory (non-recursive).
fn list_files_flat(dir: &Path, base: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if let Ok(relative) = path.strip_prefix(base) {
            let prefix = if path.is_dir() { "d " } else { "f " };
            files.push(format!("{}{}", prefix, relative.display()));
        }
    }

    files.sort();
    Ok(files)
}

/// List files in a directory (recursive).
fn list_files_recursive(dir: &Path, base: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();

    fn walk_dir(dir: &Path, base: &Path, files: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir).context("Failed to read directory")? {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            // Skip hidden files and common build directories
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
            }

            if let Ok(relative) = path.strip_prefix(base) {
                let prefix = if path.is_dir() { "d " } else { "f " };
                files.push(format!("{}{}", prefix, relative.display()));
            }

            if path.is_dir() {
                walk_dir(&path, base, files)?;
            }
        }
        Ok(())
    }

    walk_dir(dir, base, &mut files)?;
    files.sort();
    Ok(files)
}
