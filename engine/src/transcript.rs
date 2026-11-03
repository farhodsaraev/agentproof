//! Read a Claude Code session transcript (one JSON object per line).
//!
//! All knowledge of the transcript format is in this file. If Claude Code
//! changes the format, only this file must change.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::time::parse_rfc3339;

#[derive(Debug, Default, Serialize)]
pub struct Session {
    /// Prompts that the user typed.
    pub prompts: Vec<Prompt>,
    pub assistant_messages: usize,
    /// Tool calls by tool name.
    pub tool_calls: BTreeMap<String, usize>,
    pub commands: Vec<CommandRun>,
    /// Files that the agent edited or wrote with its file tools.
    pub files_edited: Vec<String>,
    pub final_message: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// Lines that are valid JSON but that this version does not use.
    pub ignored_lines: usize,
    /// Lines that are not valid JSON.
    pub bad_lines: usize,
}

#[derive(Debug, Serialize)]
pub struct Prompt {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct CommandRun {
    /// Transcript line of the tool call.
    pub line: usize,
    pub command: String,
    /// None until the result is found.
    pub failed: Option<bool>,
    pub exit_code: Option<i32>,
    pub from_subagent: bool,
}

impl Session {
    pub fn failed_commands(&self) -> usize {
        self.commands
            .iter()
            .filter(|c| c.failed == Some(true))
            .count()
    }

    pub fn duration_secs(&self) -> Option<i64> {
        Some(self.ended_at? - self.started_at?)
    }
}

pub fn read(path: &Path) -> Result<Session> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read the transcript {}", path.display()))?;
    Ok(parse(&raw))
}

const FILE_TOOLS: &[&str] = &["Edit", "MultiEdit", "Write", "NotebookEdit"];

pub fn parse(raw: &str) -> Session {
    let mut s = Session::default();
    // Tool call id -> index in `s.commands`.
    let mut pending: BTreeMap<String, usize> = BTreeMap::new();

    for (i, line) in raw.lines().enumerate() {
        let line_no = i + 1;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            s.bad_lines += 1;
            continue;
        };
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
        if kind != "user" && kind != "assistant" {
            s.ignored_lines += 1;
            continue;
        }
        if let Some(t) = entry
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339)
        {
            s.started_at.get_or_insert(t);
            s.ended_at = Some(t);
        }
        let sidechain = entry
            .get("isSidechain")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let content = entry.pointer("/message/content");

        if kind == "assistant" {
            for block in blocks(content) {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if !sidechain {
                            s.assistant_messages += 1;
                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                s.final_message = Some(t.to_string());
                            }
                        }
                    }
                    Some("tool_use") => {
                        let name = block.get("name").and_then(Value::as_str).unwrap_or("?");
                        *s.tool_calls.entry(name.to_string()).or_default() += 1;
                        let input = block.get("input");
                        if name == "Bash" || name == "PowerShell" {
                            let command = input
                                .and_then(|i| i.get("command"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            if let Some(id) = block.get("id").and_then(Value::as_str) {
                                pending.insert(id.to_string(), s.commands.len());
                            }
                            s.commands.push(CommandRun {
                                line: line_no,
                                command,
                                failed: None,
                                exit_code: None,
                                from_subagent: sidechain,
                            });
                        } else if FILE_TOOLS.contains(&name) {
                            let path = input
                                .and_then(|i| i.get("file_path").or_else(|| i.get("notebook_path")))
                                .and_then(Value::as_str);
                            if let Some(p) = path {
                                if !s.files_edited.iter().any(|f| f == p) {
                                    s.files_edited.push(p.to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        // A user entry is a typed prompt or a tool result.
        if let Some(text) = content.and_then(Value::as_str) {
            if is_typed_prompt(&entry, text, sidechain) {
                s.prompts.push(Prompt {
                    line: line_no,
                    text: text.to_string(),
                });
            }
            continue;
        }
        for block in blocks(content) {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_result") => {
                    let id = block
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(&idx) = pending.get(id) {
                        let is_error = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let cmd = &mut s.commands[idx];
                        cmd.failed = Some(is_error);
                        cmd.exit_code = if is_error {
                            exit_code(&result_text(block))
                        } else {
                            Some(0)
                        };
                    }
                }
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        if is_typed_prompt(&entry, text, sidechain) {
                            s.prompts.push(Prompt {
                                line: line_no,
                                text: text.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    s
}

fn blocks(content: Option<&Value>) -> impl Iterator<Item = &Value> {
    content.and_then(Value::as_array).into_iter().flatten()
}

fn is_typed_prompt(entry: &Value, text: &str, sidechain: bool) -> bool {
    let meta = entry
        .get("isMeta")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Slash command echoes and local command output start with a tag.
    let generated = text.starts_with("<command-") || text.starts_with("<local-command");
    !sidechain && !meta && !generated && !text.trim().is_empty()
}

fn result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Claude Code starts a failed command result with `Exit code N`.
fn exit_code(text: &str) -> Option<i32> {
    text.trim_start()
        .strip_prefix("Exit code ")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/data/session.jsonl");

    #[test]
    fn reads_prompts_and_ignores_generated_user_entries() {
        let s = parse(SAMPLE);
        let texts: Vec<_> = s.prompts.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["Fix the login bug.", "Run the tests too."]);
        assert_eq!(s.prompts[0].line, 2);
    }

    #[test]
    fn pairs_commands_with_results() {
        let s = parse(SAMPLE);
        assert_eq!(s.commands.len(), 2);
        assert_eq!(s.commands[0].command, "cargo test");
        assert_eq!(s.commands[0].failed, Some(true));
        assert_eq!(s.commands[0].exit_code, Some(101));
        assert_eq!(s.commands[1].failed, Some(false));
        assert_eq!(s.commands[1].exit_code, Some(0));
        assert_eq!(s.failed_commands(), 1);
    }

    #[test]
    fn records_edits_final_message_and_duration() {
        let s = parse(SAMPLE);
        assert_eq!(s.files_edited, ["src/login.rs"]);
        assert_eq!(s.final_message.as_deref(), Some("Done. All tests pass."));
        assert_eq!(s.tool_calls.get("Bash"), Some(&2));
        assert_eq!(s.duration_secs(), Some(95));
    }

    #[test]
    fn counts_unknown_and_bad_lines() {
        let s = parse(SAMPLE);
        assert_eq!(s.ignored_lines, 2);
        assert_eq!(s.bad_lines, 1);
    }

    #[test]
    fn reads_exit_code() {
        assert_eq!(exit_code("Exit code 49\nsome output"), Some(49));
        assert_eq!(exit_code("Error: no exit code"), None);
    }
}
