//! Read a Claude Code session transcript (one JSON object per line).
//!
//! All knowledge of the transcript format is in this file. If Claude Code
//! changes the format, only this file must change.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::testruns::{self, Exit, Summary, TestCommand, TestRun};
use crate::time::parse_rfc3339;

#[derive(Debug, Default, Serialize)]
pub struct Session {
    /// Prompts that the user typed.
    pub prompts: Vec<Prompt>,
    pub assistant_messages: usize,
    /// Tool calls by tool name.
    pub tool_calls: BTreeMap<String, usize>,
    /// The commands of the agent and of its subagents, in time order.
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
    /// The transcript line of the call that started each subagent, by agent ID.
    #[serde(skip)]
    pub agent_lines: BTreeMap<String, usize>,
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

#[derive(Debug, Default, Serialize)]
pub struct CommandRun {
    /// Transcript line of the tool call. For a command of a subagent: the line of
    /// the call that started the subagent, or 0 if the transcript does not show it.
    pub line: usize,
    pub command: String,
    /// None until the result is found, and for a command in the background
    /// until the notice at its end.
    pub failed: Option<bool>,
    pub exit_code: Option<i32>,
    /// The start of the output of a failed command.
    pub error: Option<String>,
    pub from_subagent: bool,
    /// For a command of a subagent: the line in the transcript of the subagent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_line: Option<usize>,
    /// The command ran in the background with this ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_id: Option<String>,
    /// For a test command: the runner and the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<TestRun>,
    /// For a test command: the start and the end of the output. Not in the JSON file.
    #[serde(skip)]
    pub output: Option<String>,
    /// The time of the tool call, to put the commands of subagents in order.
    #[serde(skip)]
    pub at: Option<i64>,
}

impl CommandRun {
    /// A command that ended, with no output. For the tests of other modules.
    #[cfg(test)]
    pub fn ended(line: usize, command: &str, failed: bool) -> Self {
        let test =
            testruns::detect(command).map(|t| testruns::decide(&t, Exit::Ended { failed }, None));
        CommandRun {
            line,
            command: command.into(),
            failed: Some(failed),
            exit_code: Some(i32::from(failed)),
            test,
            ..CommandRun::default()
        }
    }
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

/// Subagent transcripts are in `<session>/subagents/agent-<ID>.jsonl` next to the
/// main transcript. Their file edits and commands are also work of the agent.
fn add_subagents(s: &mut Session, path: &Path) {
    let dir = path.with_extension("").join("subagents");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let file = e.path();
        if file.extension().is_none_or(|x| x != "jsonl") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&file) else {
            continue;
        };
        let sub = parse(&raw);
        let started_at = file
            .file_stem()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("agent-"))
            .and_then(|id| s.agent_lines.get(id))
            .copied()
            .unwrap_or(0);
        for f in sub.files_edited {
            if !s.files_edited.contains(&f) {
                s.files_edited.push(f);
            }
        }
        for mut c in sub.commands {
            c.subagent_line = Some(c.line);
            c.line = started_at;
            c.from_subagent = true;
            s.commands.push(c);
        }
    }
    // Put the commands of the agent and of its subagents in time order.
    if s.commands.iter().all(|c| c.at.is_some()) {
        s.commands.sort_by_key(|c| c.at);
    } else {
        s.commands.sort_by_key(|c| c.line);
    }
}

const FILE_TOOLS: &[&str] = &["Edit", "MultiEdit", "Write", "NotebookEdit"];
const AGENT_TOOLS: &[&str] = &["Task", "Agent"];

/// What the parser knows about a test command while it reads the transcript.
struct TestState {
    command: TestCommand,
    exit: Exit,
    summary: Option<Summary>,
    output: String,
}

impl TestState {
    fn add_output(&mut self, text: &str) {
        self.summary = testruns::merge(self.summary.take(), testruns::summarize(text));
        if !self.output.is_empty() {
            self.output.push('\n');
        }
        self.output.push_str(text);
        if self.output.len() > 2 * (testruns::KEEP_HEAD + testruns::KEEP_TAIL) {
            self.output = testruns::keep(&self.output);
        }
    }
}

impl TestState {
    /// A test run that only its output shows, for example `bash run-tests.sh` or a
    /// container with tests as its command. Only a summary line with counts counts.
    fn from_output(text: &str, exit: Exit) -> Option<Self> {
        let summary = testruns::summarize(text)?;
        let runner = summary.family?.to_string();
        Some(TestState {
            // The command does not show the runner, so the exit code proves nothing.
            command: TestCommand {
                runner,
                pass_hidden: true,
                fail_hidden: true,
            },
            exit,
            summary: Some(summary),
            output: text.to_string(),
        })
    }
}

/// A command that only shows files or text, for example `cat old.log` or
/// `cd x && grep passed log`. Its output can show an old test summary.
fn is_viewer(command: &str) -> bool {
    const VIEWERS: &[&str] = &[
        "cat",
        "type",
        "more",
        "less",
        "head",
        "tail",
        "grep",
        "egrep",
        "fgrep",
        "rg",
        "ag",
        "sed",
        "awk",
        "jq",
        "find",
        "ls",
        "dir",
        "echo",
        "printf",
        "wc",
        "sort",
        "uniq",
        "cut",
        "diff",
        "git",
        "get-content",
        "gc",
        "select-string",
        "sls",
    ];
    const NEUTRAL: &[&str] = &["cd", "pushd", "popd", "export", "set"];
    let mut viewer = false;
    for part in command.split([';', '&', '|', '\n']) {
        let Some(first) = part.split_whitespace().next() else {
            continue;
        };
        let name = first
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(first)
            .trim_end_matches(".exe")
            .to_ascii_lowercase();
        if VIEWERS.contains(&name.as_str()) {
            viewer = true;
        } else if !(NEUTRAL.contains(&name.as_str())
            || first.contains('=')
            || first.chars().all(|c| c.is_ascii_digit()))
        {
            return false;
        }
    }
    viewer
}

/// A fast test for words of test summaries, before the engine reads the whole output.
fn mentions_tests(block: &Value) -> bool {
    const WORDS: &[&str] = &[
        " passed",
        " failed",
        "Tests:",
        "test result:",
        " passing",
        " failing",
        "ok  ",
    ];
    let any = |s: &str| WORDS.iter().any(|w| s.contains(w));
    match block.get("content") {
        Some(Value::String(s)) => any(s),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .any(any),
        _ => false,
    }
}

/// A tool call that reads the output files of background commands.
struct OutputRead {
    /// The Read tool puts a line number before each line.
    numbered: bool,
    /// The background ID and the index in `Session::commands` of each command.
    targets: Vec<(String, usize)>,
}

impl OutputRead {
    /// The part of the text for each command. With more than one file, `tail`
    /// and `head` put a line `==> <file> <==` before each file. Without these
    /// lines, the text cannot be divided, so no command gets it.
    fn parts<'a>(&self, text: &'a str) -> Vec<(usize, &'a str)> {
        if let [(_, idx)] = self.targets.as_slice() {
            return vec![(*idx, text)];
        }
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("==> ") {
            let header_and_body = &rest[start..];
            let Some((header, body)) = header_and_body.split_once('\n') else {
                break;
            };
            let end = body.find("\n==> ").map_or(body.len(), |p| p + 1);
            if let Some((_, idx)) = self
                .targets
                .iter()
                .find(|(bg, _)| header.contains(bg.as_str()))
            {
                out.push((*idx, &body[..end]));
            }
            rest = &body[end..];
        }
        out
    }
}

/// The text of a Read result without the line number before each line,
/// for example `12\tTests: 1 failed` or `12→Tests: 1 failed`.
fn without_line_numbers(text: &str) -> String {
    text.lines()
        .map(|l| {
            let t = l.trim_start();
            let rest = t.trim_start_matches(|c: char| c.is_ascii_digit());
            if rest.len() == t.len() {
                return l;
            }
            rest.strip_prefix('\t')
                .or_else(|| rest.strip_prefix('\u{2192}'))
                .unwrap_or(l)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A time in milliseconds, to put the commands of subagents in order.
fn millis(timestamp: &str) -> Option<i64> {
    let secs = parse_rfc3339(timestamp)?;
    let fraction: String = timestamp
        .split_once('.')
        .map(|(_, f)| f.chars().take_while(char::is_ascii_digit).take(3).collect())
        .unwrap_or_default();
    let ms = format!("{fraction:0<3}").parse::<i64>().unwrap_or(0);
    Some(secs * 1000 + ms)
}

pub fn parse(raw: &str) -> Session {
    let mut s = Session::default();
    // Tool call id -> index in `s.commands` and in `s.events`.
    let mut pending: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    // Test commands by index in `s.commands`.
    let mut tests: BTreeMap<usize, TestState> = BTreeMap::new();
    // Commands in the background: background ID -> index in `s.commands`.
    let mut background: BTreeMap<String, usize> = BTreeMap::new();
    // Tool calls that read the output of background test commands.
    let mut output_reads: BTreeMap<String, OutputRead> = BTreeMap::new();
    // Calls that start a subagent -> their transcript line.
    let mut agent_calls: BTreeMap<String, usize> = BTreeMap::new();

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
            // A notice about the end of a background command can come in an attachment.
            if let Some(text) = entry.pointer("/attachment/prompt").and_then(Value::as_str) {
                end_of_background(text, &background, &mut s.commands, &mut tests);
            }
            s.ignored_lines += 1;
            continue;
        }
        let timestamp = entry.get("timestamp").and_then(Value::as_str);
        let at = timestamp.and_then(parse_rfc3339);
        let at_ms = timestamp.and_then(millis);
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
                        let id = block.get("id").and_then(Value::as_str).unwrap_or("");
                        *s.tool_calls.entry(name.to_string()).or_default() += 1;
                        let input = block.get("input");
                        // A call that reads the output of test commands in the background.
                        if !background.is_empty() {
                            let text = input.map(Value::to_string).unwrap_or_default();
                            let targets: Vec<(String, usize)> = background
                                .iter()
                                .filter(|(bg, _)| text.contains(bg.as_str()))
                                .map(|(bg, &idx)| (bg.clone(), idx))
                                .collect();
                            if !targets.is_empty() {
                                output_reads.insert(
                                    id.to_string(),
                                    OutputRead {
                                        numbered: name == "Read",
                                        targets,
                                    },
                                );
                            }
                        }
                        if name == "Bash" || name == "PowerShell" {
                            let command = input
                                .and_then(|i| i.get("command"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let ev = push_event(&mut s, line_no, at, EventKind::Command, &command);
                            let idx = s.commands.len();
                            if !id.is_empty() {
                                pending.insert(id.to_string(), (idx, ev));
                            }
                            if let Some(t) = testruns::detect(&command) {
                                tests.insert(
                                    idx,
                                    TestState {
                                        command: t,
                                        exit: Exit::Missing,
                                        summary: None,
                                        output: String::new(),
                                    },
                                );
                            }
                            s.commands.push(CommandRun {
                                line: line_no,
                                command,
                                from_subagent: sidechain,
                                at: at_ms,
                                ..CommandRun::default()
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
                            if AGENT_TOOLS.contains(&name) {
                                agent_calls.insert(id.to_string(), line_no);
                            }
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

        // A user entry is a typed prompt, a tool result or a notice.
        if let Some(text) = content.and_then(Value::as_str) {
            end_of_background(text, &background, &mut s.commands, &mut tests);
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
                    if let Some(read) = output_reads.get(id) {
                        let mut text = result_text(block);
                        if read.numbered {
                            text = without_line_numbers(&text);
                        }
                        for (idx, part) in read.parts(&text) {
                            match tests.entry(idx) {
                                Entry::Occupied(mut e) => e.get_mut().add_output(part),
                                Entry::Vacant(e) => {
                                    let exit = Exit::Background(s.commands[idx].exit_code);
                                    if let Some(state) = TestState::from_output(part, exit) {
                                        e.insert(state);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(&call_line) = agent_calls.get(id) {
                        if let Some(agent) = agent_id(&entry, block) {
                            s.agent_lines.insert(agent, call_line);
                        }
                    }
                    let Some(&(idx, ev)) = pending.get(id) else {
                        continue;
                    };
                    if let Some(bg) = background_id(&entry, block) {
                        // The first result of a command in the background is not its result.
                        if let Some(t) = tests.get_mut(&idx) {
                            t.exit = Exit::Background(None);
                        }
                        s.commands[idx].background_id = Some(bg.clone());
                        background.insert(bg, idx);
                        continue;
                    }
                    let is_error = block
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    s.events[ev].failed = Some(is_error);
                    let is_test = tests.contains_key(&idx);
                    let text = (is_error || is_test).then(|| result_text(block));
                    let cmd = &mut s.commands[idx];
                    cmd.failed = Some(is_error);
                    if let (true, Some(text)) = (is_error, &text) {
                        cmd.exit_code = exit_code(text);
                        cmd.error = Some(error_head(text));
                    } else {
                        cmd.exit_code = Some(0);
                    }
                    // An error with no exit code is not a test failure. For example,
                    // the user rejected the call, or the command timed out.
                    let exit = if is_error && cmd.exit_code.is_none() {
                        Exit::Missing
                    } else {
                        Exit::Ended { failed: is_error }
                    };
                    if is_test {
                        if let (Some(t), Some(text)) = (tests.get_mut(&idx), text.as_deref()) {
                            t.exit = exit;
                            t.add_output(without_exit_line(text));
                        }
                    } else if !output_reads.contains_key(id)
                        && !is_viewer(&cmd.command)
                        && mentions_tests(block)
                    {
                        // A read of a background output file gives its summary to the
                        // background command. It is not a second test run.
                        // A command whose output shows a test summary also ran tests, for
                        // example `bash run-tests.sh` or a container with tests as its command.
                        let text = text.unwrap_or_else(|| result_text(block));
                        if let Some(state) = TestState::from_output(without_exit_line(&text), exit)
                        {
                            tests.insert(idx, state);
                        }
                    }
                }
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        end_of_background(text, &background, &mut s.commands, &mut tests);
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

    // The result of each test command. The timeline shows it too.
    let command_events: BTreeMap<usize, usize> = pending.values().copied().collect();
    for (idx, t) in tests {
        let run = testruns::decide(&t.command, t.exit, t.summary.as_ref());
        if let Some(&ev) = command_events.get(&idx) {
            s.events[ev].failed = match run.outcome {
                testruns::Outcome::Failed => Some(true),
                testruns::Outcome::Passed => Some(false),
                _ => None,
            };
        }
        let cmd = &mut s.commands[idx];
        cmd.output = (!t.output.is_empty()).then(|| testruns::keep(&t.output));
        cmd.test = Some(run);
    }
    s
}

/// The ID of a command that Claude Code moved to the background, from its first result:
/// `Command running in background with ID: <ID>. Output is being written to: ...`.
fn background_id(entry: &Value, block: &Value) -> Option<String> {
    if let Some(id) = entry
        .pointer("/toolUseResult/backgroundTaskId")
        .and_then(Value::as_str)
    {
        return Some(id.to_string());
    }
    let start = match block.get("content") {
        Some(Value::String(s)) => s.as_str(),
        Some(Value::Array(parts)) => parts
            .iter()
            .find_map(|p| p.get("text").and_then(Value::as_str))
            .unwrap_or(""),
        _ => "",
    };
    let rest = start
        .trim_start()
        .strip_prefix("Command running in background with ID: ")?;
    let id: String = rest
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    (!id.is_empty()).then_some(id)
}

/// Claude Code tells the agent when a background command ends:
/// `<task-notification><task-id>ID</task-id>...<status>completed</status>
/// <summary>... (exit code 1)</summary></task-notification>`.
fn end_of_background(
    text: &str,
    background: &BTreeMap<String, usize>,
    commands: &mut [CommandRun],
    tests: &mut BTreeMap<usize, TestState>,
) {
    if background.is_empty() || !text.contains("<task-notification>") {
        return;
    }
    for notice in text.split("<task-notification>").skip(1) {
        let Some(&idx) =
            between(notice, "<task-id>", "</task-id>").and_then(|id| background.get(id.trim()))
        else {
            continue;
        };
        let status = between(notice, "<status>", "</status>").unwrap_or("");
        if !matches!(status.trim(), "completed" | "failed") {
            continue;
        }
        // "completed (exit code 0)" or "failed with exit code 1".
        let code = between(notice, "<summary>", "</summary>")
            .and_then(|s| s.rsplit_once("exit code "))
            .and_then(|(_, rest)| {
                let n: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '-')
                    .collect();
                n.parse::<i32>().ok()
            })
            .or_else(|| (status.trim() == "failed").then_some(1));
        if let Some(code) = code {
            commands[idx].exit_code = Some(code);
            commands[idx].failed = Some(code != 0);
        }
        if let Some(t) = tests.get_mut(&idx) {
            t.exit = Exit::Background(code);
        }
    }
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &text[text.find(start)? + start.len()..];
    Some(&rest[..rest.find(end)?])
}

/// The agent ID of a subagent, from the result of the call that started it.
fn agent_id(entry: &Value, block: &Value) -> Option<String> {
    if let Some(id) = entry
        .pointer("/toolUseResult/agentId")
        .and_then(Value::as_str)
    {
        return Some(id.to_string());
    }
    let text = result_text(block);
    let rest = &text[text.find("agentId: ")? + "agentId: ".len()..];
    let id: String = rest
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    (!id.is_empty()).then_some(id)
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
    // Slash command echoes, local command output and notices start with a tag.
    let generated = text.starts_with("<command-")
        || text.starts_with("<local-command")
        || text.trim_start().starts_with("<task-notification>");
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

/// The output without the `Exit code N` line at the start of a failed result.
fn without_exit_line(text: &str) -> &str {
    match text.trim_start().strip_prefix("Exit code ") {
        Some(rest) => rest.split_once('\n').map_or("", |(_, b)| b),
        None => text,
    }
}

/// The first 500 characters of a failed result, without the `Exit code N` line.
fn error_head(text: &str) -> String {
    without_exit_line(text).trim().chars().take(500).collect()
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
    use crate::testruns::{Basis, Outcome};

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

    #[test]
    fn a_piped_test_run_gets_its_result_from_the_summary() {
        let s = parse(include_str!("../tests/data/piped_failure.jsonl"));
        let c = &s.commands[0];
        assert_eq!(c.failed, Some(false), "the pipe hid the exit code");
        let t = c.test.as_ref().unwrap();
        assert_eq!((t.outcome, t.basis), (Outcome::Failed, Basis::Summary));
        assert!(t.exit_code_hidden);
        assert_eq!(
            t.summary.as_deref(),
            Some("Tests:       2 failed, 0 passed, 2 total")
        );
        assert!(c
            .output
            .as_deref()
            .unwrap()
            .contains("FAIL tests/math.test.js"));
        let ev = s
            .events
            .iter()
            .find(|e| e.kind == EventKind::Command)
            .unwrap();
        assert_eq!(ev.failed, Some(true), "the timeline shows the failure");
    }

    const BG_CALL: &str = r#"{"type":"assistant","timestamp":"2026-11-03T10:00:01.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"b1","name":"Bash","input":{"command":"npm test","run_in_background":true}}]}}"#;
    const BG_STARTED: &str = r#"{"type":"user","timestamp":"2026-11-03T10:00:02.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"b1","is_error":false,"content":"Command running in background with ID: bx1y2z. Output is being written to: C:\\tmp\\tasks\\bx1y2z.output. You will be notified when it completes."}]},"toolUseResult":{"stdout":"","stderr":"","backgroundTaskId":"bx1y2z"}}"#;
    const BG_DONE: &str = r#"{"type":"user","timestamp":"2026-11-03T10:00:09.000Z","message":{"role":"user","content":"<task-notification>\n<task-id>bx1y2z</task-id>\n<tool-use-id>b1</tool-use-id>\n<status>completed</status>\n<summary>Background command \"npm test\" completed (exit code 0)</summary>\n</task-notification>"}}"#;
    const BG_READ: &str = r#"{"type":"assistant","timestamp":"2026-11-03T10:00:10.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"r1","name":"Read","input":{"file_path":"C:\\tmp\\tasks\\bx1y2z.output"}}]}}"#;
    // The Read tool puts a line number before each line.
    const BG_OUTPUT: &str = r#"{"type":"user","timestamp":"2026-11-03T10:00:11.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"r1","content":"1\tFAIL src/a.test.js\n2\tTests:       1 failed, 3 passed, 4 total"}]}}"#;
    const CLAIM: &str = r#"{"type":"assistant","timestamp":"2026-11-03T10:00:12.000Z","message":{"role":"assistant","content":[{"type":"text","text":"All tests pass."}]}}"#;

    #[test]
    fn a_background_test_run_needs_its_summary() {
        let s = parse(&[BG_CALL, BG_STARTED, BG_DONE, CLAIM].join("\n"));
        let c = &s.commands[0];
        assert_eq!(c.background_id.as_deref(), Some("bx1y2z"));
        assert_eq!(c.exit_code, Some(0), "from the notice at the end");
        let t = c.test.as_ref().unwrap();
        assert_eq!(
            (t.outcome, t.basis),
            (Outcome::Unknown, Basis::Background),
            "exit code 0 from the background is not a pass"
        );
        assert!(s.prompts.is_empty(), "a notice is not a prompt");

        let s = parse(&[BG_CALL, BG_STARTED, BG_DONE, BG_READ, BG_OUTPUT, CLAIM].join("\n"));
        let t = s.commands[0].test.as_ref().unwrap();
        assert_eq!((t.outcome, t.basis), (Outcome::Failed, Basis::Summary));
    }

    fn bash(id: &str, command: &str) -> String {
        serde_json::json!({"type": "assistant", "timestamp": "2026-11-03T10:00:01.000Z",
            "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id,
            "name": "Bash", "input": {"command": command}}]}})
        .to_string()
    }

    fn result(id: &str, is_error: bool, text: &str) -> String {
        serde_json::json!({"type": "user", "timestamp": "2026-11-03T10:00:02.000Z",
            "message": {"role": "user", "content": [{"type": "tool_result",
            "tool_use_id": id, "is_error": is_error, "content": text}]}})
        .to_string()
    }

    #[test]
    fn notices_in_both_forms_and_in_attachments_count() {
        // Claude Code writes "failed with exit code 1", with no brackets.
        let failed = BG_DONE
            .replace("<status>completed</status>", "<status>failed</status>")
            .replace("completed (exit code 0)", "failed with exit code 1");
        let s = parse(&[BG_CALL, BG_STARTED, &failed, CLAIM].join("\n"));
        let t = s.commands[0].test.as_ref().unwrap();
        assert_eq!((t.outcome, t.basis), (Outcome::Failed, Basis::ExitCode));
        assert_eq!(s.commands[0].failed, Some(true));

        // The same notice in an attachment entry.
        let notice = "<task-notification>\n<task-id>bx1y2z</task-id>\n<status>failed</status>\n<summary>Background command \"npm test\" failed with exit code 1</summary>\n</task-notification>";
        let attachment = serde_json::json!({"type": "attachment", "attachment": {
            "type": "queued_command", "commandMode": "task-notification", "prompt": notice}})
        .to_string();
        let s = parse(&[BG_CALL, BG_STARTED, &attachment, CLAIM].join("\n"));
        assert_eq!(
            s.commands[0].test.as_ref().unwrap().outcome,
            Outcome::Failed
        );
    }

    #[test]
    fn a_rejected_test_command_has_no_result() {
        let s = parse(
            &[
                bash("t1", "npm test"),
                result(
                    "t1",
                    true,
                    "The user doesn't want to proceed with this tool use. The tool use was rejected.",
                ),
            ]
            .join("\n"),
        );
        let t = s.commands[0].test.as_ref().unwrap();
        assert_eq!((t.outcome, t.basis), (Outcome::Unknown, Basis::NoResult));
    }

    #[test]
    fn one_read_of_two_outputs_is_divided_by_file() {
        let lines = [
            bash("u1", "npm run test:unit"),
            result(
                "u1",
                false,
                "Command running in background with ID: bunit01. Output is being written to: /tmp/tasks/bunit01.output",
            ),
            bash("e1", "npm run test:e2e"),
            result(
                "e1",
                false,
                "Command running in background with ID: be2e002. Output is being written to: /tmp/tasks/be2e002.output",
            ),
            bash(
                "r1",
                "tail -n 3 /tmp/tasks/bunit01.output /tmp/tasks/be2e002.output",
            ),
            result(
                "r1",
                false,
                "==> /tmp/tasks/bunit01.output <==\nTests:       4 passed, 4 total\n\n==> /tmp/tasks/be2e002.output <==\nFAIL e2e/login.test.js\nTests:       1 failed, 2 passed, 3 total",
            ),
        ];
        let s = parse(&lines.join("\n"));
        let outcome = |i: usize| s.commands[i].test.as_ref().unwrap().outcome;
        assert_eq!(outcome(0), Outcome::Passed);
        assert_eq!(outcome(1), Outcome::Failed);
    }

    #[test]
    fn a_command_whose_output_shows_a_test_summary_is_a_test_run() {
        let summary = "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s";
        let s = parse(
            &[
                bash("s1", "bash scripts/test-all.sh"),
                result("s1", false, summary),
            ]
            .join("\n"),
        );
        let t = s.commands[0].test.as_ref().unwrap();
        assert_eq!(
            (t.runner.as_str(), t.outcome, t.basis),
            ("cargo test", Outcome::Passed, Basis::Summary)
        );

        // A file viewer can show an old summary, so it is not a test run.
        let s = parse(&[bash("v1", "cat old.log"), result("v1", false, summary)].join("\n"));
        assert!(s.commands[0].test.is_none());

        // The output file of a background command that runs the tests in a container.
        let lines = [
            bash(
                "b1",
                "docker compose --profile test run --rm backend-test > log.txt 2>&1",
            ),
            result(
                "b1",
                false,
                "Command running in background with ID: bk7x2q. Output is being written to: /tmp/tasks/bk7x2q.output",
            ),
            bash("c1", "cat /tmp/tasks/bk7x2q.output; ssh host uptime"),
            result(
                "c1",
                false,
                "334 passed, 1 warning in 359.48s (0:05:59)\n\n[exited with code 0]",
            ),
        ];
        let s = parse(&lines.join("\n"));
        let t = s.commands[0].test.as_ref().unwrap();
        assert_eq!((t.runner.as_str(), t.outcome), ("pytest", Outcome::Passed));
        assert!(
            s.commands[1].test.is_none(),
            "a read of the output file is not a second test run"
        );
    }

    #[test]
    fn times_keep_their_milliseconds() {
        let base = parse_rfc3339("2026-11-03T10:00:05Z").unwrap() * 1000;
        assert_eq!(millis("2026-11-03T10:00:05.100Z"), Some(base + 100));
        assert_eq!(millis("2026-11-03T10:00:05Z"), Some(base));
        assert_eq!(
            without_line_numbers("     1\tok\n2\u{2192}FAIL x\nno number"),
            "ok\nFAIL x\nno number"
        );
    }

    #[test]
    fn subagent_commands_join_the_session_in_time_order() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main.jsonl");
        std::fs::write(&main, include_str!("../tests/data/subagent_failure.jsonl")).unwrap();
        let sub = tmp.path().join("main/subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("agent-a1b2c3.jsonl"),
            include_str!("../tests/data/subagent_failure/subagents/agent-a1b2c3.jsonl"),
        )
        .unwrap();
        let s = read(&main).unwrap();
        assert_eq!(s.commands.len(), 1);
        let c = &s.commands[0];
        assert_eq!(c.command, "npx jest");
        assert!(c.from_subagent);
        assert_eq!(c.line, 4, "the line of the call that started the subagent");
        assert_eq!(c.subagent_line, Some(1));
        assert_eq!(c.test.as_ref().unwrap().outcome, Outcome::Failed);
    }
}
