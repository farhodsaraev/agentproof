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
    pub final_message_line: Option<usize>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// Lines that are valid JSON but that this version does not use.
    pub ignored_lines: usize,
    /// Lines that are not valid JSON.
    pub bad_lines: usize,
    /// All steps in order. Not in the JSON file, because the report shows them.
    #[serde(skip)]
    pub events: Vec<Event>,
    /// Commands of subagents. Their transcripts are separate files.
    #[serde(skip)]
    pub subagent_commands: Vec<String>,
}

/// One step of the session, for the timeline in the report.
#[derive(Debug, Clone)]
pub struct Event {
    pub line: usize,
    pub at: Option<i64>,
    pub kind: EventKind,
    /// Shortened text: the prompt, the message, the command or the tool input.
    pub text: String,
    /// For commands: true if the command failed.
    pub failed: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Prompt,
    Message,
    Command,
    Edit,
    Tool,
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
    /// The start of the output of a failed command.
    pub error: Option<String>,
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
    let mut s = parse(&raw);
    add_subagents(&mut s, path);
    Ok(s)
}

/// Subagent transcripts are in `<session>/subagents/*.jsonl` next to the main
/// transcript. Their file edits and commands are also work of the agent.
fn add_subagents(s: &mut Session, path: &Path) {
    let dir = path.with_extension("").join("subagents");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        if e.path().extension().is_none_or(|x| x != "jsonl") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let sub = parse(&raw);
        for f in sub.files_edited {
            if !s.files_edited.contains(&f) {
                s.files_edited.push(f);
            }
        }
        s.subagent_commands
            .extend(sub.commands.into_iter().map(|c| c.command));
    }
}

const FILE_TOOLS: &[&str] = &["Edit", "MultiEdit", "Write", "NotebookEdit"];

pub fn parse(raw: &str) -> Session {
    let mut s = Session::default();
    // Tool call id -> index in `s.commands` and in `s.events`.
    let mut pending: BTreeMap<String, (usize, usize)> = BTreeMap::new();

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
        let at = entry
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339);
        if let Some(t) = at {
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
                                s.final_message_line = Some(line_no);
                                push_event(&mut s, line_no, at, EventKind::Message, t);
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
                            let ev = push_event(&mut s, line_no, at, EventKind::Command, &command);
                            if let Some(id) = block.get("id").and_then(Value::as_str) {
                                pending.insert(id.to_string(), (s.commands.len(), ev));
                            }
                            s.commands.push(CommandRun {
                                line: line_no,
                                command,
                                failed: None,
                                exit_code: None,
                                error: None,
                                from_subagent: sidechain,
                            });
                        } else if FILE_TOOLS.contains(&name) {
                            let path = input
                                .and_then(|i| i.get("file_path").or_else(|| i.get("notebook_path")))
                                .and_then(Value::as_str);
                            if let Some(p) = path {
                                push_event(
                                    &mut s,
                                    line_no,
                                    at,
                                    EventKind::Edit,
                                    &format!("{name} {p}"),
                                );
                                if !s.files_edited.iter().any(|f| f == p) {
                                    s.files_edited.push(p.to_string());
                                }
                            }
                        } else {
                            // Other tools: show the name and the main input.
                            let detail = input
                                .and_then(|i| {
                                    [
                                        "file_path",
                                        "path",
                                        "pattern",
                                        "url",
                                        "query",
                                        "description",
                                        "skill",
                                    ]
                                    .iter()
                                    .find_map(|k| i.get(*k).and_then(Value::as_str))
                                })
                                .unwrap_or("");
                            push_event(
                                &mut s,
                                line_no,
                                at,
                                EventKind::Tool,
                                &format!("{name} {detail}"),
                            );
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
                push_event(&mut s, line_no, at, EventKind::Prompt, text);
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
                    if let Some(&(idx, ev)) = pending.get(id) {
                        let is_error = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        s.events[ev].failed = Some(is_error);
                        let cmd = &mut s.commands[idx];
                        cmd.failed = Some(is_error);
                        if is_error {
                            let text = result_text(block);
                            cmd.exit_code = exit_code(&text);
                            cmd.error = Some(error_head(&text));
                        } else {
                            cmd.exit_code = Some(0);
                        }
                    }
                }
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        if is_typed_prompt(&entry, text, sidechain) {
                            push_event(&mut s, line_no, at, EventKind::Prompt, text);
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

const EVENT_TEXT_MAX: usize = 400;

fn push_event(s: &mut Session, line: usize, at: Option<i64>, kind: EventKind, text: &str) -> usize {
    s.events.push(Event {
        line,
        at,
        kind,
        text: text.trim().chars().take(EVENT_TEXT_MAX).collect(),
        failed: None,
    });
    s.events.len() - 1
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

/// The first 500 characters of a failed result, without the `Exit code N` line.
fn error_head(text: &str) -> String {
    let body = match text.trim_start().strip_prefix("Exit code ") {
        Some(rest) => rest.split_once('\n').map_or("", |(_, b)| b),
        None => text,
    };
    body.trim().chars().take(500).collect()
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
