//! Rule checks: facts from the transcript and the changes, with no AI.
//!
//! Each check gives findings with a severity, a location and evidence.
//! No location, no finding. The text of each finding is in `messages.rs`.

use std::path::Path;

use serde::Serialize;

use crate::changes::{FileDiff, StartPoint};
use crate::testruns::{covers, selects_all, Basis, Outcome, TestRun};
use crate::transcript::{CommandRun, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum Rule {
    TestFileDeleted,
    TestsRemoved {
        count: usize,
    },
    AssertionsRemoved {
        count: usize,
    },
    TestSkipped {
        marker: String,
    },
    TestFocused {
        marker: String,
    },
    TestsFailedButClaimedPass,
    TestsFailedAtEnd,
    /// A test run failed, and later only a run with fewer tests passed.
    NarrowPassAfterFailure,
    PassClaimWithoutTests {
        phrase: String,
    },
    /// The agent said that the tests pass, but no test run passed.
    PassClaimUnverified {
        phrase: String,
        reason: Unverified,
    },
    DangerousCommand {
        kind: Danger,
    },
    DependenciesChanged,
    SensitiveFile {
        area: Area,
    },
    SuppressionsAdded {
        count: usize,
        marker: String,
    },
    LargeDeletion {
        lines: usize,
    },
    OutsideRequest {
        count: usize,
        folders: Vec<String>,
    },
    RepeatedError {
        count: usize,
    },
    /// AI opinion: the evidence shows that a claim of the agent is false.
    AiContradicted {
        claim: String,
        reason: String,
    },
    /// AI opinion: the evidence does not show that a claim of the agent is true.
    AiUnsupported {
        claim: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Danger {
    RecursiveDelete,
    ForcePush,
    HardReset,
    CleanUntracked,
    PipeToShell,
    Sudo,
}

/// Why no test run supports a claim that the tests pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unverified {
    /// The last test run found no tests.
    NoTests,
    /// The shell hid the exit code, and the output has no test summary.
    HiddenExitCode,
    /// The tests ran in the background, and the transcript has no result.
    Background,
    /// The transcript has no result for the test run.
    NoResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    Authentication,
    Payments,
    Migrations,
    Ci,
    Secrets,
    AgentConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    #[serde(flatten)]
    pub rule: Rule,
    pub severity: Severity,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub transcript_line: Option<usize>,
    /// The text that caused the finding, shortened.
    pub evidence: String,
    /// The ASD-STE100 text for users. `check.rs` fills it.
    pub text: String,
    /// The agent did not touch this file, so a different session or a person
    /// possibly made the change. The severity is then Low.
    #[serde(default)]
    pub other_origin: bool,
    /// The request named this file, so the change is expected. The severity is then Low.
    #[serde(default)]
    pub named_in_request: bool,
    /// The command ran in a subagent.
    #[serde(default)]
    pub in_subagent: bool,
    /// The index of the command in `Session::commands`. Not in the JSON file.
    #[serde(skip)]
    pub command: Option<usize>,
}

impl Finding {
    /// A finding with no location and no evidence.
    pub fn new(rule: Rule, severity: Severity) -> Self {
        Finding {
            rule,
            severity,
            file: None,
            line: None,
            transcript_line: None,
            evidence: String::new(),
            text: String::new(),
            other_origin: false,
            named_in_request: false,
            in_subagent: false,
            command: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    DontTrust,
    Check,
    NoRedFlags,
    /// A source was missing, or the engine cannot trust it, so some checks did not run.
    Incomplete,
}

/// Thresholds. These become settings in `.agentproof.toml` later.
pub const LARGE_DELETION_LINES: usize = 200;
pub const REPEATED_ERROR_COUNT: usize = 3;

pub struct Input<'a> {
    pub session: Option<&'a Session>,
    pub diffs: &'a [FileDiff],
    /// The project folder, to resolve paths that the user named.
    pub root: Option<&'a Path>,
    /// The recorded start point. None if the session has no start point.
    pub start: Option<&'a StartPoint>,
}

pub fn evaluate(input: &Input) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Some(s) = input.session {
        test_runs(s, &mut out);
        dangerous_commands(s, &mut out);
        repeated_errors(s, &mut out);
    }
    for d in input.diffs {
        test_edits(d, &mut out);
        sensitive_file(d, &mut out);
        suppressions(d, &mut out);
        large_deletion(d, &mut out);
    }
    dependencies(input.diffs, &mut out);
    if let (Some(s), Some(root)) = (input.session, input.root) {
        outside_request(s, input.diffs, root, &mut out);
    }
    // Git does not know who made a change. A change to a file that the agent
    // did not touch can come from a different session or from a person.
    if let Some(s) = input.session {
        for f in &mut out {
            let other = f.transcript_line.is_none()
                && f.file
                    .as_deref()
                    .is_some_and(|file| !changed_by_agent(s, file));
            if other {
                f.severity = Severity::Low;
                f.other_origin = true;
            }
        }
    }
    // A recursive delete of a folder that the session made is cleanup, not a danger.
    let recursive_delete = Rule::DangerousCommand {
        kind: Danger::RecursiveDelete,
    };
    if let (Some(s), Some(sp)) = (input.session, input.start) {
        if out.iter().any(|f| f.rule == recursive_delete) {
            if let Some(at_start) = crate::changes::paths_at_start(sp) {
                out.retain(|f| {
                    !(f.rule == recursive_delete
                        && f.command.is_some_and(|i| {
                            deletes_only_session_folders(s, &at_start, i, input.root)
                        }))
                });
            }
        }
    }
    // A sensitive file that the request names is expected work.
    if let (Some(s), Some(root)) = (input.session, input.root) {
        let named = named_paths(s, root);
        for f in &mut out {
            let requested = matches!(f.rule, Rule::SensitiveFile { .. })
                && f.file.as_deref().is_some_and(|file| {
                    let file = file.to_lowercase();
                    named.iter().any(|n| n.to_lowercase() == file)
                });
            if requested {
                f.severity = Severity::Low;
                f.named_in_request = true;
            }
        }
    }
    out.sort_by_key(|f| f.severity);
    out
}

/// Commands that can change files without naming them, for example a package
/// manager or a formatter. After such a command, any file can be work of the agent.
const BROAD_COMMANDS: &[&str] = &[
    "install",
    " add ",
    "update",
    "upgrade",
    "fmt",
    "format",
    "prettier",
    "black ",
    "ruff",
    "--fix",
    "generate",
    "codegen",
    "migrate",
    "make",
    "go get",
    "go mod",
    "patch ",
    "git checkout",
    "git switch",
    "git apply",
    "git am",
    "git merge",
    "git pull",
    "git rebase",
    "git stash",
    "git restore",
    "git reset",
    "git cherry-pick",
    "git revert",
];

/// True if the agent possibly changed the file: it edited the file with a file
/// tool, named the file in a command, or ran a command that can change any file.
pub fn changed_by_agent(s: &Session, path: &str) -> bool {
    let rel = path.replace('\\', "/").to_lowercase();
    let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
    let edited = s.files_edited.iter().any(|e| {
        let e = e.replace('\\', "/").to_lowercase();
        let e = e.trim_start_matches("./");
        e == rel || e.ends_with(&format!("/{rel}"))
    });
    // The commands of subagents count too.
    let commands = s.commands.iter().map(|c| c.command.as_str());
    edited
        || commands.into_iter().any(|c| {
            let c = c.replace('\\', "/").to_lowercase();
            c.contains(&rel)
                || (name.len() >= 3 && c.contains(&name))
                || BROAD_COMMANDS.iter().any(|b| c.contains(b))
        })
}

/// The verdict. The best verdict is "no red flags found", also when the AI check
/// supports each claim: an opinion of a model is not a proof. A later proof run
/// can give "safe to merge".
pub fn verdict(findings: &[Finding], complete: bool) -> Verdict {
    if findings.iter().any(|f| f.severity == Severity::Critical) {
        Verdict::DontTrust
    } else if !complete {
        Verdict::Incomplete
    } else if findings.iter().any(|f| f.severity <= Severity::Medium) {
        Verdict::Check
    } else {
        Verdict::NoRedFlags
    }
}

fn short(text: &str) -> String {
    // Evidence goes into the JSON file and the report, so it must not show a secret.
    let clean = crate::redact::redact(text).0;
    let t = clean.trim();
    if t.chars().count() > 200 {
        format!("{}...", t.chars().take(200).collect::<String>())
    } else {
        t.to_string()
    }
}

/// A finding at the command with the index `i` in `Session::commands`.
fn at_command(rule: Rule, severity: Severity, i: usize, c: &CommandRun) -> Finding {
    Finding {
        // A command of a subagent has line 0 if the transcript does not show its start.
        transcript_line: (c.line > 0).then_some(c.line),
        evidence: short(&c.command),
        in_subagent: c.from_subagent,
        command: Some(i),
        ..Finding::new(rule, severity)
    }
}

fn at_file(
    rule: Rule,
    severity: Severity,
    file: &str,
    line: Option<u32>,
    evidence: &str,
) -> Finding {
    Finding {
        file: Some(file.to_string()),
        line,
        evidence: short(evidence),
        ..Finding::new(rule, severity)
    }
}

// ---------- Tests in the transcript ----------

/// Words that make a sentence about tests negative or uncertain.
const NOT_A_PASS: &[&str] = &[
    "not", "no", "never", "fail", "failed", "fails", "failing", "skip", "skipped", "without",
    "cannot", "couldn", "didn", "don", "doesn", "isn", "aren", "wasn", "weren", "won", "should",
    "will", "would", "may", "might", "if", "before", "except", "flaky",
];

/// Words that name tests.
const TEST_WORDS: &[&str] = &["test", "tests", "tested", "testing", "testsuite"];

/// Words for success. "successful" is not in the list, because "the successful
/// login path" is a description, not a claim.
const PASS_WORDS: &[&str] = &[
    "pass",
    "passes",
    "passed",
    "passing",
    "green",
    "succeed",
    "succeeds",
    "succeeded",
    "successfully",
];

/// The words of a part of a sentence. "haven't" gives "have" and "not".
fn clause_words(clause: &str) -> Vec<String> {
    clause
        .replace("n't", " not")
        .replace("n\u{2019}t", " not")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The sentences in the text that say that the tests pass, each with the parts of
/// the sentence that make the claim. One part of the sentence must have a word for
/// tests and a word for success, and no negative or uncertain word. A part with a
/// word for tests and a negative word cancels the claim, for example "except one
/// flaky test".
fn claims(text: &str) -> impl Iterator<Item = (&str, Vec<Vec<String>>)> {
    fn has(words: &[String], list: &[&str]) -> bool {
        words.iter().any(|w| list.contains(&w.as_str()))
    }
    text.split(['.', '!', '?', '\n', ';'])
        .filter_map(|sentence| {
            let lower = [" but ", " although ", " though ", " however ", ":"]
                .iter()
                .fold(sentence.to_lowercase(), |s, sep| s.replace(sep, ","));
            let parts: Vec<Vec<String>> = lower.split(',').map(clause_words).collect();
            let cancel = parts
                .iter()
                .any(|p| has(p, TEST_WORDS) && has(p, NOT_A_PASS));
            let claim: Vec<Vec<String>> = parts
                .into_iter()
                .filter(|p| has(p, TEST_WORDS) && has(p, PASS_WORDS) && !has(p, NOT_A_PASS))
                .collect();
            (!claim.is_empty() && !cancel).then_some((sentence, claim))
        })
}

/// The first sentence in the text that says that the tests pass, if any. The
/// sentence is shortened to 12 words.
pub fn pass_claim(text: &str) -> Option<String> {
    let (sentence, _) = claims(text).next()?;
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let mut out = words.iter().take(12).copied().collect::<Vec<_>>().join(" ");
    if words.len() > 12 {
        out.push('…');
    }
    Some(out)
}

/// True if the text says that all tests pass, for example "all tests pass", "all
/// 105 tests passed" or "the test suite is green". A claim such as "the new login
/// test passes" is about some tests only. Only the part of a sentence that makes
/// the claim counts, and a general word counts only near the word for tests.
fn general_claim(text: &str) -> bool {
    const GENERAL: &[&str] = &["all", "every", "entire", "whole", "full"];
    claims(text).any(|(_, parts)| {
        parts.iter().any(|p| {
            p.iter().any(|w| w == "suite")
                || p.iter().enumerate().any(|(i, w)| {
                    TEST_WORDS.contains(&w.as_str())
                        && p[i.saturating_sub(4)..i]
                            .iter()
                            .any(|g| GENERAL.contains(&g.as_str()))
                })
        })
    })
}

fn test_runs(s: &Session, out: &mut Vec<Finding>) {
    let claim = s.final_message.as_deref().and_then(pass_claim);
    let general = s.final_message.as_deref().is_some_and(general_claim);
    // The test commands of the agent and of its subagents, in time order.
    let runs: Vec<(usize, &CommandRun, &TestRun)> = s
        .commands
        .iter()
        .enumerate()
        .filter_map(|(i, c)| Some((i, c, c.test.as_ref()?)))
        .collect();

    if runs.is_empty() {
        if let (Some(phrase), Some(line)) = (claim.as_ref(), s.final_message_line) {
            out.push(Finding {
                transcript_line: Some(line),
                evidence: short(s.final_message.as_deref().unwrap_or("")),
                ..Finding::new(
                    Rule::PassClaimWithoutTests {
                        phrase: phrase.to_string(),
                    },
                    Severity::High,
                )
            });
        }
        return;
    }

    // A failed run counts only if no later run of the same runner passed with the
    // same tests or more tests. A narrower run, for example one test file, does not
    // clear the failure of a full run.
    let later_passes = |j: usize, t: &TestRun| {
        runs[j + 1..]
            .iter()
            .filter(|(_, _, l)| l.runner == t.runner && l.outcome == Outcome::Passed)
            .map(|(_, _, l)| *l)
            .collect::<Vec<_>>()
    };
    let unresolved = runs.iter().enumerate().rev().find_map(|(j, (i, c, t))| {
        let cleared = || later_passes(j, t).iter().any(|l| covers(l, t));
        (t.outcome == Outcome::Failed && !cleared()).then_some((j, *i, *c, *t))
    });
    if let Some((j, i, c, t)) = unresolved {
        // A later run passed with a part of the failed tests.
        let narrower_pass = later_passes(j, t).iter().any(|l| selects_all(t, l));
        let (rule, severity) = match &claim {
            // "All tests pass" after a run of a part of the tests is still a false claim.
            Some(_) if !narrower_pass || general => {
                (Rule::TestsFailedButClaimedPass, Severity::Critical)
            }
            _ if narrower_pass => (Rule::NarrowPassAfterFailure, Severity::High),
            _ => (Rule::TestsFailedAtEnd, Severity::High),
        };
        let mut f = at_command(rule, severity, i, c);
        if let Some(summary) = &t.summary {
            // No run times: the evidence is part of the key of the finding.
            f.evidence = short(&format!(
                "{} → {}",
                c.command,
                crate::testruns::without_times(summary)
            ));
        }
        out.push(f);
        return;
    }

    // A claim that the tests pass needs a run that passed.
    let Some(phrase) = claim else { return };
    if runs.iter().any(|(_, _, t)| t.outcome == Outcome::Passed) {
        return;
    }
    let Some(&(i, c, t)) = runs.last() else {
        return;
    };
    let reason = match (t.outcome, t.basis) {
        (Outcome::NoTests, _) => Unverified::NoTests,
        (_, Basis::HiddenExitCode) => Unverified::HiddenExitCode,
        (_, Basis::Background) => Unverified::Background,
        _ => Unverified::NoResult,
    };
    out.push(at_command(
        Rule::PassClaimUnverified { phrase, reason },
        Severity::High,
        i,
        c,
    ));
}

// ---------- Dangerous commands ----------

/// Build, cache and generated folders that are safe to delete.
const SAFE_DELETE: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".cache",
    "__pycache__",
    ".pytest_cache",
    "coverage",
    ".turbo",
    "tmp",
    "temp",
    ".venv",
    "venv",
    "generated",
    "__generated__",
];

/// Parts of a path that show a temporary folder.
const TEMP_PLACES: &[&str] = &[
    "/tmp/",
    "/temp/",
    "appdata/local/temp",
    "scratchpad",
    "$temp",
    "${temp}",
    "$tmp",
    "${tmp}",
    "$tmpdir",
    "${tmpdir}",
    "%temp%",
    "/var/folders/",
];

const DOWNLOADERS: &[&str] = &[
    "curl",
    "wget",
    "iwr",
    "irm",
    "invoke-webrequest",
    "invoke-restmethod",
];
const SHELLS: &[&str] = &["sh", "bash", "zsh", "iex", "invoke-expression"];

pub fn danger(command: &str) -> Option<Danger> {
    let lower = strip_heredocs(command).to_lowercase();
    let squashed: String = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    if DOWNLOADERS.iter().any(|d| squashed.contains(d)) && pipes_to_shell(&squashed) {
        return Some(Danger::PipeToShell);
    }
    let vars = assignments(&squashed);
    for segment in squashed.split(['\n', ';', '&', '|']) {
        let mut words: Vec<&str> = segment.split_whitespace().collect();
        if words.first() == Some(&"sudo") {
            return Some(Danger::Sudo);
        }
        while words
            .first()
            .is_some_and(|w| *w == "env" || (w.contains('=') && !w.starts_with('-')))
        {
            words.remove(0);
        }
        match words.as_slice() {
            ["rm", rest @ ..] => {
                let flags: String = rest
                    .iter()
                    .filter(|w| w.starts_with('-') && !w.starts_with("--"))
                    .flat_map(|w| w.chars())
                    .collect();
                let recursive = flags.contains('r') || rest.contains(&"--recursive");
                let force = flags.contains('f') || rest.contains(&"--force");
                if recursive && force && !only_safe_targets(rest, &vars) {
                    return Some(Danger::RecursiveDelete);
                }
            }
            ["remove-item", rest @ ..] | ["rd", rest @ ..] | ["rmdir", rest @ ..] => {
                let recursive = rest.iter().any(|w| *w == "-recurse" || *w == "/s");
                if recursive && !only_safe_targets(rest, &vars) {
                    return Some(Danger::RecursiveDelete);
                }
            }
            ["git", "push", rest @ ..] => {
                if rest.iter().any(|w| {
                    *w == "-f" || *w == "--force" || w.starts_with("--force-") || w.starts_with('+')
                }) {
                    return Some(Danger::ForcePush);
                }
            }
            ["git", "reset", rest @ ..] if rest.contains(&"--hard") => {
                return Some(Danger::HardReset)
            }
            ["git", "clean", rest @ ..]
                if rest
                    .iter()
                    .any(|w| w.starts_with('-') && !w.starts_with("--") && w.contains('f'))
                    || rest.contains(&"--force") =>
            {
                return Some(Danger::CleanUntracked)
            }
            _ => {}
        }
    }
    None
}

/// True if the command sends output into a shell with one pipe (`| sh`).
/// `a || bash b.sh` is not a pipe: it runs a local script when `a` fails.
fn pipes_to_shell(cmd: &str) -> bool {
    let b = cmd.as_bytes();
    (0..b.len()).any(|i| {
        if b[i] != b'|' || (i > 0 && b[i - 1] == b'|') || b.get(i + 1) == Some(&b'|') {
            return false;
        }
        let rest = cmd[i + 1..].trim_start();
        SHELLS.iter().any(|s| {
            rest.strip_prefix(s)
                .is_some_and(|after| after.is_empty() || after.starts_with([' ', ';', '-']))
        })
    })
}

/// Simple variables that the command sets, such as `S=/tmp/x` or `W="$TEMP/y"`.
fn assignments(cmd: &str) -> Vec<(String, String)> {
    let mut vars = Vec::new();
    for part in cmd.split(['\n', ';', '&', '|', ' ']) {
        let part = part.trim_start_matches("export");
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        let valid = !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if valid {
            vars.push((
                name.to_string(),
                value.trim_matches(['"', '\'']).to_string(),
            ));
        }
    }
    // Longer names first, so that `$sx` is not replaced as `$s` + `x`.
    vars.sort_by_key(|(n, _)| std::cmp::Reverse(n.len()));
    vars
}

fn expand(target: &str, vars: &[(String, String)]) -> String {
    let mut t = target.to_string();
    for (name, value) in vars {
        t = t
            .replace(&format!("${{{name}}}"), value)
            .replace(&format!("${name}"), value);
    }
    t
}

/// The paths in the arguments of a delete command, with variables expanded.
/// Options and redirections such as `2>` are not paths.
fn targets_of(args: &[&str], vars: &[(String, String)]) -> Vec<String> {
    args.iter()
        .filter(|w| !w.starts_with('-') && !w.contains('>') && !w.contains('<'))
        .map(|w| expand(w.trim_matches(['"', '\'']), vars).replace('\\', "/"))
        .filter(|w| !w.is_empty())
        .collect()
}

/// The targets of all recursive deletes in a command (lowercase).
pub fn delete_targets(command: &str) -> Vec<String> {
    let lower = strip_heredocs(command).to_lowercase();
    let squashed: String = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    let vars = assignments(&squashed);
    let mut out = Vec::new();
    for segment in squashed.split(['\n', ';', '&', '|']) {
        let mut words: Vec<&str> = segment.split_whitespace().collect();
        while words.first().is_some_and(|w| {
            *w == "env" || *w == "sudo" || (w.contains('=') && !w.starts_with('-'))
        }) {
            words.remove(0);
        }
        if let ["rm" | "remove-item" | "rd" | "rmdir", rest @ ..] = words.as_slice() {
            out.extend(targets_of(rest, &vars));
        }
    }
    out
}

/// Remove the bodies of here-documents (`cat > f <<'EOF' ... EOF`). They are
/// file content, not commands.
pub(crate) fn strip_heredocs(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut lines = command.lines();
    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        let Some(pos) = line.find("<<") else { continue };
        let rest = &line[pos + 2..];
        if rest.starts_with('<') {
            continue; // `<<<` is a here-string on one line.
        }
        let word: String = rest
            .trim_start_matches('-')
            .trim_start()
            .trim_start_matches(['\'', '"'])
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if word.is_empty() {
            continue;
        }
        for body in lines.by_ref() {
            if body.trim() == word {
                out.push_str(body);
                out.push('\n');
                break;
            }
        }
    }
    out
}

/// True if every folder that the command deletes did not exist when the session
/// started, and the session used it before. Such a delete is cleanup, not a danger.
fn deletes_only_session_folders(
    s: &Session,
    at_start: &[String],
    i: usize,
    root: Option<&Path>,
) -> bool {
    let Some(cmd) = s.commands.get(i) else {
        return false;
    };
    let lower = cmd.command.to_lowercase();
    let targets = delete_targets(&cmd.command);
    !targets.is_empty()
        && targets.iter().all(|t| {
            let t = relative_to(t, root);
            let t = t.trim_start_matches("./").trim_end_matches('/');
            let relative = !t.is_empty()
                && !t.starts_with(['/', '~', '$', '%'])
                && !t.contains([':', '*', '?'])
                && !t.split('/').any(|p| p == "..");
            if !relative {
                return false;
            }
            let existed = at_start
                .iter()
                .any(|p| p == t || p.starts_with(&format!("{t}/")));
            // The session used the folder before: in an earlier command, earlier in
            // the same command, or with a file tool.
            let used_before = s.commands[..i]
                .iter()
                .any(|c| c.command.to_lowercase().contains(t))
                || lower.find("rm ").is_some_and(|i| lower[..i].contains(t))
                || s.files_edited.iter().any(|e| {
                    let e = e.replace('\\', "/").to_lowercase();
                    e.starts_with(&format!("{t}/")) || e.contains(&format!("/{t}/"))
                });
            !existed && used_before
        })
}

/// A delete target inside the repository as a path relative to its root. Git
/// Bash writes the folder `C:/repo` as `/c/repo`. Other targets do not change.
fn relative_to(target: &str, root: Option<&Path>) -> String {
    let t = target.replace('\\', "/");
    let Some(root) = root else {
        return t;
    };
    let root = root.to_string_lossy().replace('\\', "/").to_lowercase();
    let root = root.trim_end_matches('/');
    let git_bash = match root.split_once(":/") {
        Some((drive, rest)) if drive.len() == 1 => format!("/{drive}/{rest}"),
        _ => root.to_string(),
    };
    [root.to_string(), git_bash]
        .iter()
        .find_map(|r| t.strip_prefix(&format!("{r}/")))
        .map_or(t.clone(), str::to_string)
}

fn only_safe_targets(args: &[&str], vars: &[(String, String)]) -> bool {
    let targets = targets_of(args, vars);
    !targets.is_empty()
        && targets.iter().all(|t| {
            let t = t.trim_end_matches('/');
            let parts: Vec<&str> = t.split('/').filter(|p| !p.is_empty()).collect();
            if parts.contains(&"..") {
                return false;
            }
            let in_temp = TEMP_PLACES.iter().any(|p| format!("{t}/").contains(p));
            let in_build = parts.iter().any(|p| SAFE_DELETE.contains(p));
            // A temporary folder itself, or something inside a temporary or build folder.
            (in_temp || in_build) && parts.len() >= 2
                || parts.len() == 1 && SAFE_DELETE.contains(&parts[0])
        })
}

fn dangerous_commands(s: &Session, out: &mut Vec<Finding>) {
    for (i, c) in s.commands.iter().enumerate() {
        if let Some(kind) = danger(&c.command) {
            out.push(at_command(
                Rule::DangerousCommand { kind },
                Severity::Critical,
                i,
                c,
            ));
        }
    }
}

// ---------- Repeated errors ----------

/// The first line of an error, with numbers removed, so that similar errors match.
fn error_key(error: &str) -> Option<String> {
    let line = error.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.chars().filter(|c| !c.is_ascii_digit()).collect())
}

fn repeated_errors(s: &Session, out: &mut Vec<Finding>) {
    let mut seen: Vec<(String, usize, usize)> = Vec::new();
    for (i, c) in s.commands.iter().enumerate() {
        if c.failed != Some(true) {
            continue;
        }
        let Some(key) = c.error.as_deref().and_then(error_key) else {
            continue;
        };
        match seen.iter_mut().find(|(k, _, _)| *k == key) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 = i;
            }
            None => seen.push((key, 1, i)),
        }
    }
    for (key, count, last) in seen {
        if count >= REPEATED_ERROR_COUNT {
            let mut f = at_command(
                Rule::RepeatedError { count },
                Severity::Low,
                last,
                &s.commands[last],
            );
            f.evidence = short(&key);
            out.push(f);
        }
    }
}

// ---------- Tests in the changes ----------

pub fn is_test_file(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    let name = p.rsplit('/').next().unwrap_or(&p);
    let dirs = ["test/", "tests/", "__tests__/", "spec/", "specs/"];
    dirs.iter()
        .any(|d| p.starts_with(d) || p.contains(&format!("/{d}")))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.contains("_test.")
        || name.contains("_spec.")
        || name.starts_with("test_")
        || name.ends_with("test.java")
        || name.ends_with("tests.cs")
        || name.ends_with("test.kt")
}

/// Markers that skip tests. The bool is true if the marker is test-only in any file.
const SKIP_MARKERS: &[(&str, bool)] = &[
    ("#[ignore", true),
    ("@pytest.mark.skip", true),
    ("@unittest.skip", true),
    ("pytest.skip(", true),
    ("@disabled", true),
    ("@ignore", false),
    ("t.skip(", false),
    (".skip(", false),
    ("xit(", false),
    ("xdescribe(", false),
    ("xtest(", false),
];

const FOCUS_MARKERS: &[&str] = &[".only(", "fit(", "fdescribe(", "ftest("];

const TEST_DEFS: &[&str] = &[
    "#[test]",
    "#[tokio::test",
    "def test_",
    "func test",
    "it(",
    "test(",
    "@test",
];

const ASSERTIONS: &[&str] = &[
    "assert", "expect(", ".should", "require.", "t.error", "t.fatal",
];

fn has_marker(line: &str, marker: &str) -> bool {
    let l = line.trim_start().to_lowercase();
    // Attributes and decorators must start the line. In a string they are only text.
    if marker.starts_with("#[")
        || marker.starts_with("#![")
        || (marker.starts_with('@') && !marker.starts_with("@ts-"))
    {
        return l.starts_with(marker);
    }
    if l.starts_with("//") {
        return false;
    }
    if marker.ends_with('(') && marker.chars().next().is_some_and(char::is_alphabetic) {
        // A function name such as `xit(` must not be part of a longer word.
        return l.match_indices(marker).any(|(i, _)| {
            outside_string(&l, i)
                && l[..i]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        });
    }
    contains_outside_string(&l, marker)
}

/// True if byte `i` of the line is not inside a "double-quoted" string.
fn outside_string(line: &str, i: usize) -> bool {
    let mut quotes = 0;
    let mut escaped = false;
    for c in line[..i].chars() {
        match c {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => {
                quotes += 1;
                escaped = false;
            }
            _ => escaped = false,
        }
    }
    quotes % 2 == 0
}

/// A marker in a string, such as a list of markers, is only text.
fn contains_outside_string(line: &str, marker: &str) -> bool {
    line.match_indices(marker)
        .any(|(i, _)| outside_string(line, i))
}

fn count_lines(lines: &[(u32, String)], markers: &[&str]) -> usize {
    lines
        .iter()
        .filter(|(_, t)| markers.iter().any(|m| has_marker(t, m)))
        .count()
}

fn test_edits(d: &FileDiff, out: &mut Vec<Finding>) {
    let test_file = is_test_file(&d.path);
    let first_removed = d.removed.first().map(|(n, t)| (*n, t.as_str()));

    if d.status == 'D' && test_file {
        let (line, text) = first_removed.unwrap_or((1, ""));
        out.push(at_file(
            Rule::TestFileDeleted,
            Severity::High,
            &d.path,
            Some(line),
            text,
        ));
        return;
    }

    for (n, text) in &d.added {
        if let Some((m, _)) = SKIP_MARKERS
            .iter()
            .find(|(m, any_file)| (test_file || *any_file) && has_marker(text, m))
        {
            out.push(at_file(
                Rule::TestSkipped {
                    marker: shown_marker(m),
                },
                Severity::High,
                &d.path,
                Some(*n),
                text,
            ));
        } else if let Some(m) = FOCUS_MARKERS
            .iter()
            .find(|m| test_file && has_marker(text, m))
        {
            out.push(at_file(
                Rule::TestFocused {
                    marker: shown_marker(m),
                },
                Severity::High,
                &d.path,
                Some(*n),
                text,
            ));
        }
    }

    let is_rust = d.path.ends_with(".rs");
    if !(test_file || is_rust) {
        return;
    }
    let removed_tests =
        count_lines(&d.removed, TEST_DEFS).saturating_sub(count_lines(&d.added, TEST_DEFS));
    if removed_tests > 0 {
        let (line, text) = d
            .removed
            .iter()
            .find(|(_, t)| TEST_DEFS.iter().any(|m| has_marker(t, m)))
            .map(|(n, t)| (*n, t.as_str()))
            .unwrap_or((0, ""));
        out.push(at_file(
            Rule::TestsRemoved {
                count: removed_tests,
            },
            Severity::High,
            &d.path,
            Some(line),
            text,
        ));
    } else if test_file {
        let removed =
            count_lines(&d.removed, ASSERTIONS).saturating_sub(count_lines(&d.added, ASSERTIONS));
        if removed > 0 {
            let (line, text) = d
                .removed
                .iter()
                .find(|(_, t)| ASSERTIONS.iter().any(|m| has_marker(t, m)))
                .map(|(n, t)| (*n, t.as_str()))
                .unwrap_or((0, ""));
            out.push(at_file(
                Rule::AssertionsRemoved { count: removed },
                Severity::High,
                &d.path,
                Some(line),
                text,
            ));
        }
    }
}

// ---------- Dependencies ----------

const MANIFESTS: &[&str] = &[
    "cargo.toml",
    "package.json",
    "pyproject.toml",
    "pipfile",
    "setup.py",
    "setup.cfg",
    "go.mod",
    "gemfile",
    "composer.json",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "packages.config",
    "deno.json",
];

const LOCKFILES: &[(&str, &str)] = &[
    ("cargo.lock", "cargo.toml"),
    ("package-lock.json", "package.json"),
    ("yarn.lock", "package.json"),
    ("pnpm-lock.yaml", "package.json"),
    ("bun.lockb", "package.json"),
    ("poetry.lock", "pyproject.toml"),
    ("uv.lock", "pyproject.toml"),
    ("pipfile.lock", "pipfile"),
    ("go.sum", "go.mod"),
    ("gemfile.lock", "gemfile"),
    ("composer.lock", "composer.json"),
];

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

fn dependencies(diffs: &[FileDiff], out: &mut Vec<Finding>) {
    for d in diffs {
        let name = file_name(&d.path);
        let is_manifest = MANIFESTS.contains(&name.as_str())
            || name.starts_with("requirements") && name.ends_with(".txt")
            || name.ends_with(".csproj");
        let lock_of = LOCKFILES.iter().find(|(l, _)| *l == name).map(|(_, m)| *m);
        // A lock file counts only if its manifest in the same folder did not change.
        let lock_alone = lock_of.is_some_and(|m| {
            !diffs
                .iter()
                .any(|o| dir_of(&o.path) == dir_of(&d.path) && file_name(&o.path) == m)
        });
        if is_manifest || lock_alone {
            let (line, text) = d
                .added
                .first()
                .or(d.removed.first())
                .map(|(n, t)| (Some(*n), t.as_str()))
                .unwrap_or((None, ""));
            out.push(at_file(
                Rule::DependenciesChanged,
                Severity::Medium,
                &d.path,
                line,
                text,
            ));
        }
    }
}

// ---------- Sensitive files ----------

pub fn sensitive_area(path: &str) -> Option<Area> {
    let p = path.replace('\\', "/").to_lowercase();
    let name = p.rsplit('/').next().unwrap_or(&p);
    if name == "claude.md"
        || name == "agents.md"
        || name == ".agentproof.toml"
        || p.starts_with(".claude/")
        || p.contains("/.claude/")
        || p.starts_with(".cursor/")
    {
        return Some(Area::AgentConfig);
    }
    if p.starts_with(".github/workflows/")
        || name == ".gitlab-ci.yml"
        || p.starts_with(".circleci/")
        || name == "jenkinsfile"
        || name == "azure-pipelines.yml"
    {
        return Some(Area::Ci);
    }
    if name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.starts_with("id_rsa")
    {
        return Some(Area::Secrets);
    }
    let words: Vec<&str> = p.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let has = |set: &[&str]| words.iter().any(|w| set.contains(w));
    if has(&[
        "auth",
        "authentication",
        "login",
        "logout",
        "password",
        "passwords",
        "oauth",
        "jwt",
        "permissions",
        "rbac",
    ]) {
        Some(Area::Authentication)
    } else if has(&[
        "payment",
        "payments",
        "billing",
        "stripe",
        "checkout",
        "invoice",
        "invoices",
        "subscription",
        "subscriptions",
    ]) {
        Some(Area::Payments)
    } else if has(&["migration", "migrations", "migrate"]) {
        Some(Area::Migrations)
    } else if has(&["secret", "secrets", "credential", "credentials"]) {
        Some(Area::Secrets)
    } else {
        None
    }
}

fn sensitive_file(d: &FileDiff, out: &mut Vec<Finding>) {
    if let Some(area) = sensitive_area(&d.path) {
        let (line, text) = d
            .added
            .first()
            .or(d.removed.first())
            .map(|(n, t)| (Some(*n), t.as_str()))
            .unwrap_or((None, ""));
        out.push(at_file(
            Rule::SensitiveFile { area },
            Severity::Medium,
            &d.path,
            line,
            text,
        ));
    }
}

// ---------- Suppressions ----------

const SUPPRESSIONS: &[&str] = &[
    " as any",
    "<any>",
    "@ts-ignore",
    "@ts-expect-error",
    "@ts-nocheck",
    "eslint-disable",
    "# type: ignore",
    "# noqa",
    "# pylint: disable",
    "#[allow(",
    "#![allow(",
    "//nolint",
    "// nolint",
    "@suppresswarnings",
    "#pragma warning disable",
    "istanbul ignore",
    "c8 ignore",
    "pragma: no cover",
    "// biome-ignore",
    "#[expect(",
    "# nosec",
];

fn suppression_at(line: &str, marker: &str) -> bool {
    let l = line.to_lowercase();
    if marker.starts_with("#[")
        || marker.starts_with("#![")
        || (marker.starts_with('@') && !marker.starts_with("@ts-"))
    {
        l.trim_start().starts_with(marker)
    } else {
        contains_outside_string(&l, marker)
    }
}

/// Documents can name a marker as text. Only code can suppress a warning.
fn is_document(path: &str) -> bool {
    let p = path.to_lowercase();
    [".md", ".markdown", ".txt", ".rst", ".adoc", ".org"]
        .iter()
        .any(|ext| p.ends_with(ext))
}

fn suppressions(d: &FileDiff, out: &mut Vec<Finding>) {
    if is_document(&d.path) {
        return;
    }
    let hits: Vec<&(u32, String)> = d
        .added
        .iter()
        .filter(|(_, t)| SUPPRESSIONS.iter().any(|m| suppression_at(t, m)))
        .collect();
    if let Some((n, text)) = hits.first() {
        let marker = SUPPRESSIONS
            .iter()
            .find(|m| suppression_at(text, m))
            .unwrap_or(&"")
            .trim()
            .to_string();
        out.push(at_file(
            Rule::SuppressionsAdded {
                count: hits.len(),
                marker: shown_marker(&marker),
            },
            Severity::Medium,
            &d.path,
            Some(*n),
            text,
        ));
    }
}

// ---------- Large deletions ----------

fn large_deletion(d: &FileDiff, out: &mut Vec<Finding>) {
    let lines = d.removed.len();
    if lines >= LARGE_DELETION_LINES {
        let (n, text) = &d.removed[0];
        out.push(at_file(
            Rule::LargeDeletion { lines },
            Severity::Medium,
            &d.path,
            Some(*n),
            text,
        ));
    }
}

// ---------- Changes outside the request ----------

/// Folders and files that the user named in the prompts and that exist.
fn named_paths(s: &Session, root: &Path) -> Vec<String> {
    let mut named = Vec::new();
    for p in &s.prompts {
        for word in p
            .text
            .split(|c: char| c.is_whitespace() || "`'\"(),;<>[]{}".contains(c))
        {
            let w = word
                .trim_end_matches(['.', '!', '?', ':'])
                .replace('\\', "/");
            // `src/a.rs:12` names the file `src/a.rs`.
            let w = match w.rsplit_once(':') {
                Some((p, n)) if n.chars().all(|c| c.is_ascii_digit()) => p.to_string(),
                _ => w,
            };
            let w = w.trim_start_matches("./").trim_end_matches('/');
            // Only relative paths inside the project. On Windows, a path that
            // starts with `//` is a network share, and a check of it is very slow.
            if w.is_empty() || w.starts_with('/') || w.contains(':') || w.contains("..") {
                continue;
            }
            let looks_like_path = w.contains('/')
                || w.rsplit_once('.').is_some_and(|(a, b)| {
                    !a.is_empty()
                        && (1..=5).contains(&b.len())
                        && b.chars().all(|c| c.is_ascii_alphanumeric())
                });
            if looks_like_path
                && !w.contains("://")
                && root.join(w).exists()
                && !named.iter().any(|n| n == w)
            {
                named.push(w.to_string());
            }
        }
    }
    named
}

/// The changed files outside the folders that the request named, and those
/// folders. None if the request named no paths, because then there is nothing
/// to compare.
pub fn outside_request_files<'a>(
    s: &Session,
    diffs: &'a [FileDiff],
    root: &Path,
) -> Option<(Vec<&'a FileDiff>, Vec<String>)> {
    let named = named_paths(s, root);
    if named.is_empty() {
        return None;
    }
    // A named file allows its folder. A named folder allows everything in it.
    let allowed: Vec<String> = named
        .iter()
        .map(|n| {
            if root.join(n).is_dir() {
                n.clone()
            } else {
                dir_of(n).to_string()
            }
        })
        .collect();
    let inside = |path: &str| {
        allowed.iter().any(|a| {
            a.is_empty() && !path.contains('/')
                || !a.is_empty() && (path == a || path.starts_with(&format!("{a}/")))
        }) || named.iter().any(|n| n == path)
    };
    let outside = diffs.iter().filter(|d| !inside(&d.path)).collect();
    let mut folders: Vec<String> = allowed
        .iter()
        .map(|a| if a.is_empty() { ".".into() } else { a.clone() })
        .collect();
    folders.dedup();
    Some((outside, folders))
}

fn outside_request(s: &Session, diffs: &[FileDiff], root: &Path, out: &mut Vec<Finding>) {
    let Some((outside, folders)) = outside_request_files(s, diffs, root) else {
        return;
    };
    if let Some(first) = outside.first() {
        let evidence = outside
            .iter()
            .take(5)
            .map(|d| d.path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let line = first
            .added
            .first()
            .or(first.removed.first())
            .map(|(n, _)| *n);
        out.push(at_file(
            Rule::OutsideRequest {
                count: outside.len(),
                folders,
            },
            Severity::Low,
            &first.path,
            line,
            &evidence,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(line: usize, command: &str, failed: bool, error: Option<&str>) -> CommandRun {
        CommandRun {
            error: error.map(Into::into),
            ..CommandRun::ended(line, command, failed)
        }
    }

    fn session(commands: Vec<CommandRun>, final_message: &str) -> Session {
        Session {
            commands,
            final_message: Some(final_message.into()),
            final_message_line: Some(99),
            ..Session::default()
        }
    }

    fn diff(path: &str, added: &[&str], removed: &[&str]) -> FileDiff {
        let lines = |v: &[&str]| {
            v.iter()
                .enumerate()
                .map(|(i, t)| (i as u32 + 1, t.to_string()))
                .collect()
        };
        FileDiff {
            path: path.into(),
            status: 'M',
            added: lines(added),
            removed: lines(removed),
        }
    }

    fn rules(input: &Input) -> Vec<Rule> {
        evaluate(input).into_iter().map(|f| f.rule).collect()
    }

    fn only_session(s: &Session) -> Vec<Rule> {
        rules(&Input {
            session: Some(s),
            diffs: &[],
            root: None,
            start: None,
        })
    }

    fn only_diffs(d: &[FileDiff]) -> Vec<Rule> {
        rules(&Input {
            session: None,
            diffs: d,
            root: None,
            start: None,
        })
    }

    fn from_file(text: &str) -> Vec<Finding> {
        let s = crate::transcript::parse(text);
        evaluate(&Input {
            session: Some(&s),
            diffs: &[],
            root: None,
            start: None,
        })
    }

    #[test]
    fn a_piped_failure_with_a_pass_claim_is_critical() {
        // Experiment e1: `npx jest 2>&1 | tail -20` hid the exit code of a failure.
        let f = from_file(include_str!("../tests/data/piped_failure.jsonl"));
        assert_eq!(f[0].rule, Rule::TestsFailedButClaimedPass);
        assert_eq!(f[0].severity, Severity::Critical);
        assert!(f[0].evidence.contains("2 failed"), "{}", f[0].evidence);
    }

    #[test]
    fn a_run_with_no_tests_does_not_resolve_a_failure() {
        // Experiment e2: a filtered run found no tests after a full run failed.
        let f = from_file(include_str!("../tests/data/no_tests_after_failure.jsonl"));
        assert_eq!(f[0].rule, Rule::TestsFailedButClaimedPass);
        assert_eq!(f[0].severity, Severity::Critical);
    }

    #[test]
    fn a_failure_in_a_subagent_counts() {
        // Experiment e7: a subagent ran the tests, and they failed.
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
        let s = crate::transcript::read(&main).unwrap();
        let f = evaluate(&Input {
            session: Some(&s),
            diffs: &[],
            root: None,
            start: None,
        });
        assert_eq!(f[0].rule, Rule::TestsFailedButClaimedPass);
        assert_eq!(f[0].severity, Severity::Critical);
        assert!(f[0].in_subagent);
        assert_eq!(f[0].transcript_line, Some(4));
    }

    #[test]
    fn a_later_pass_in_a_script_resolves_a_failure() {
        let call = |id: &str, command: &str| {
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "tool_use", "id": id, "name": "Bash",
                "input": {"command": command}}]}})
            .to_string()
        };
        let result = |id: &str, is_error: bool, text: &str| {
            serde_json::json!({"type": "user", "message": {"role": "user",
                "content": [{"type": "tool_result", "tool_use_id": id,
                "is_error": is_error, "content": text}]}})
            .to_string()
        };
        let say = serde_json::json!({"type": "assistant", "message": {"role": "assistant",
            "content": [{"type": "text", "text": "Fixed. All tests pass."}]}})
        .to_string();
        let lines = [
            call("t1", "cargo test"),
            result(
                "t1",
                true,
                "Exit code 101\ntest result: FAILED. 1 passed; 1 failed; 0 ignored;",
            ),
            call("t2", "bash scripts/check.sh"),
            result(
                "t2",
                false,
                "test result: ok. 2 passed; 0 failed; 0 ignored;",
            ),
            say,
        ];
        assert!(from_file(&lines.join("\n")).is_empty());
    }

    /// A transcript with test commands. Each command has its error mark and its
    /// output. The agent says the last text.
    fn transcript(commands: &[(&str, bool, &str)], text: &str) -> String {
        let mut lines = Vec::new();
        for (n, (command, is_error, output)) in commands.iter().enumerate() {
            let id = format!("t{n}");
            lines.push(
                serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                    "content": [{"type": "tool_use", "id": id, "name": "Bash",
                    "input": {"command": command}}]}})
                .to_string(),
            );
            lines.push(
                serde_json::json!({"type": "user", "message": {"role": "user",
                    "content": [{"type": "tool_result", "tool_use_id": id,
                    "is_error": is_error, "content": output}]}})
                .to_string(),
            );
        }
        lines.push(
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "text", "text": text}]}})
            .to_string(),
        );
        lines.join("\n")
    }

    #[test]
    fn a_script_run_of_one_test_file_does_not_resolve_a_full_failure() {
        let f = from_file(&transcript(
            &[
                (
                    "pytest",
                    true,
                    "Exit code 1\n===== 1 failed, 99 passed in 3.00s =====",
                ),
                (
                    "./scripts/test.sh tests/test_dates.py",
                    false,
                    "===== 5 passed in 0.20s =====",
                ),
            ],
            "All tests pass.",
        ));
        assert_eq!(f[0].rule, Rule::TestsFailedButClaimedPass);
    }

    #[test]
    fn a_run_in_another_folder_does_not_resolve_a_failure() {
        let fail = "Exit code 1\nTests: 1 failed, 4 passed, 5 total";
        let pass = "Tests: 6 passed, 6 total";
        let f = from_file(&transcript(
            &[
                ("cd api && npm test", true, fail),
                ("cd web && npm test", false, pass),
            ],
            "All tests pass.",
        ));
        assert_eq!(f[0].rule, Rule::TestsFailedButClaimedPass);
        // The shell stays in the folder of the last `cd`, so a later run with no
        // `cd` can be in the same folder.
        let f = from_file(&transcript(
            &[
                ("cd api && npm test", true, fail),
                ("npm test", false, pass),
            ],
            "All tests pass.",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn reading_a_test_log_is_not_a_test_run() {
        let f = from_file(&transcript(
            &[(
                r#"$L = "C:\tmp\e2e.log"; Select-String -Path $L -Pattern 'EXIT|passed|failed'"#,
                false,
                "  66 failed\n  12 passed (2.1m)",
            )],
            "I read the log.",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn a_pass_claim_after_a_test_script_is_not_a_finding() {
        let s = session(
            vec![cmd(3, "npm run test:unit", false, None)],
            "Done. All tests pass.",
        );
        assert!(only_session(&s).is_empty());
    }

    #[test]
    fn a_pass_claim_needs_a_visible_pass() {
        let claim = |c: CommandRun| only_session(&session(vec![c], "All tests pass."));
        let unverified = |reason| {
            vec![Rule::PassClaimUnverified {
                phrase: "All tests pass".into(),
                reason,
            }]
        };
        assert_eq!(
            claim(CommandRun::ended(3, "npm test 2>&1 | head -5", false)),
            unverified(Unverified::HiddenExitCode)
        );
        let mut c = CommandRun::ended(3, "npm test", false);
        if let Some(t) = c.test.as_mut() {
            t.outcome = Outcome::Unknown;
            t.basis = Basis::Background;
        }
        assert_eq!(claim(c), unverified(Unverified::Background));
        let mut c = CommandRun::ended(3, "npm test", false);
        if let Some(t) = c.test.as_mut() {
            t.outcome = Outcome::NoTests;
            t.basis = Basis::Summary;
        }
        assert_eq!(claim(c), unverified(Unverified::NoTests));
    }

    #[test]
    fn finds_pass_claims_but_not_negations() {
        assert_eq!(
            pass_claim("Done. All tests pass.").as_deref(),
            Some("All tests pass")
        );
        assert_eq!(
            pass_claim("The tests are green now").as_deref(),
            Some("The tests are green now")
        );
        assert_eq!(
            pass_claim("I created the file. I also ran all tests and they pass.").as_deref(),
            Some("I also ran all tests and they pass")
        );
        assert_eq!(
            pass_claim("Run the tests before you merge, so that they pass."),
            None
        );
        assert_eq!(pass_claim("Tests: 12 passed, 1 failed."), None);
        assert_eq!(pass_claim("Not all tests pass yet."), None);
        assert_eq!(pass_claim("The tests don't pass."), None);
        assert_eq!(pass_claim("I updated the docs."), None);
        assert_eq!(pass_claim("The tests haven't passed yet."), None);
        assert_eq!(pass_claim("I can\u{2019}t say that the tests pass."), None);
        assert_eq!(pass_claim("I added tests for the passport form."), None);
        assert_eq!(pass_claim("The testimonial page passes review."), None);
        // A negative word in another part of the sentence does not cancel a claim.
        assert!(pass_claim(
            "Fixed the bug and all tests pass, but I haven't committed the change."
        )
        .is_some());
        assert_eq!(
            pass_claim("I added a test for the successful login path."),
            None
        );
        assert_eq!(pass_claim("All tests pass, except one flaky test."), None);
        assert_eq!(
            pass_claim("The unit tests passed.").as_deref(),
            Some("The unit tests passed")
        );
    }

    #[test]
    fn failed_tests_with_a_pass_claim_are_critical() {
        let s = session(
            vec![cmd(5, "cargo test", true, Some("test x ... FAILED"))],
            "Done. All tests pass.",
        );
        let f = evaluate(&Input {
            session: Some(&s),
            diffs: &[],
            root: None,
            start: None,
        });
        assert_eq!(f[0].rule, Rule::TestsFailedButClaimedPass);
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[0].transcript_line, Some(5));
    }

    #[test]
    fn failed_tests_without_a_claim_are_high() {
        let s = session(
            vec![cmd(5, "npm test", true, None)],
            "Some tests still fail.",
        );
        assert_eq!(only_session(&s), [Rule::TestsFailedAtEnd]);
    }

    #[test]
    fn a_later_passing_run_resolves_a_failure() {
        // The later run has the same tests or more tests.
        for (failed, passed) in [
            ("cargo test", "cargo test"),
            ("cargo test --lib", "cargo test"),
            ("pytest tests/test_a.py::test_x", "pytest tests/test_a.py"),
            ("pytest", "pytest tests/"),
            // The later run drops a name filter.
            (
                "pytest tests/test_auth.py -k test_login",
                "pytest tests/test_auth.py",
            ),
            ("go test ./pkg/dates -run TestParse", "go test ./pkg/dates"),
            ("cargo test --lib parse_date", "cargo test --lib"),
            // The later run names a module that holds the earlier tests.
            (
                "python manage.py test blog.tests.test_models",
                "python manage.py test blog",
            ),
            (
                "python -m unittest tests.test_auth",
                "python -m unittest discover",
            ),
            (
                "bundle exec rspec spec/user_spec.rb:42",
                "bundle exec rspec spec/user_spec.rb",
            ),
            ("cargo test parse_empty_input", "cargo test parse"),
        ] {
            let s = session(
                vec![cmd(5, failed, true, None), cmd(9, passed, false, None)],
                "All tests pass.",
            );
            assert!(only_session(&s).is_empty(), "{failed} -> {passed}");
        }
    }

    #[test]
    fn a_narrower_passing_run_does_not_resolve_a_failure() {
        let narrow = |claim: &str| {
            only_session(&session(
                vec![
                    cmd(5, "npx jest", true, None),
                    cmd(9, "npx jest tests/math.test.js", false, None),
                ],
                claim,
            ))
        };
        // A claim about all tests is false.
        assert_eq!(
            narrow("Fixed. All tests pass."),
            [Rule::TestsFailedButClaimedPass]
        );
        // A claim about one test can be true, so the finding is High.
        assert_eq!(
            narrow("The math test passes now."),
            [Rule::NarrowPassAfterFailure]
        );
        assert_eq!(
            narrow("I changed the add function."),
            [Rule::NarrowPassAfterFailure]
        );
        // The later run adds a name filter, or it names one file.
        for (failed, passed) in [
            (
                "pytest tests/test_auth.py",
                "pytest tests/test_auth.py -k test_login",
            ),
            ("cargo test -p engine", "cargo test -p engine parse_date"),
            ("go test", "go test -run TestFormat"),
            ("pytest", "pytest -v tests/test_math.py"),
            ("npm test", "npm test src/auth.test.js"),
            ("cargo test", "cargo test -- --ignored"),
            ("go test ./pkg/auth/...", "go test ./pkg/auth"),
        ] {
            let s = |claim: &str| {
                only_session(&session(
                    vec![cmd(5, failed, true, None), cmd(9, passed, false, None)],
                    claim,
                ))
            };
            assert_eq!(
                s("All tests pass."),
                [Rule::TestsFailedButClaimedPass],
                "{failed} -> {passed}"
            );
        }
    }

    #[test]
    fn a_pass_of_other_tests_keeps_a_claim_critical() {
        let s = session(
            vec![
                cmd(5, "pytest tests/test_a.py", true, None),
                cmd(9, "pytest tests/test_b.py", false, None),
            ],
            "The tests pass now.",
        );
        assert_eq!(only_session(&s), [Rule::TestsFailedButClaimedPass]);
        let s = session(
            vec![
                cmd(5, "pytest tests/test_a.py", true, None),
                cmd(9, "pytest tests/test_b.py", false, None),
            ],
            "I changed the parser.",
        );
        assert_eq!(only_session(&s), [Rule::TestsFailedAtEnd]);
    }

    #[test]
    fn a_general_claim_is_in_the_part_that_makes_the_claim() {
        assert!(general_claim("The whole test suite is green."));
        assert!(general_claim("All 105 tests pass."));
        assert!(general_claim(
            "I fixed the off-by-one error in the parse_date helper function, and now all tests pass."
        ));
        assert!(general_claim(
            "The new regression test passes. All other tests pass too."
        ));
        assert!(!general_claim("The new lock test passes."));
        assert!(!general_claim(
            "The login test passes, but I did not run the full suite."
        ));
        assert!(!general_claim(
            "I addressed all the review comments and the updated login test passes."
        ));
    }

    #[test]
    fn a_pass_claim_without_a_test_run_is_high() {
        let s = session(
            vec![cmd(3, "cargo build", false, None)],
            "Fixed. Tests pass.",
        );
        assert_eq!(
            only_session(&s),
            [Rule::PassClaimWithoutTests {
                phrase: "Tests pass".into()
            }]
        );
    }

    #[test]
    fn finds_dangerous_commands() {
        assert_eq!(danger("rm -rf src"), Some(Danger::RecursiveDelete));
        assert_eq!(danger("rm -r -f ~/project"), Some(Danger::RecursiveDelete));
        assert_eq!(
            danger("Remove-Item -Recurse -Force C:\\work"),
            Some(Danger::RecursiveDelete)
        );
        assert_eq!(
            danger("git push --force origin main"),
            Some(Danger::ForcePush)
        );
        assert_eq!(danger("git push -f"), Some(Danger::ForcePush));
        assert_eq!(danger("git push origin +main"), Some(Danger::ForcePush));
        assert_eq!(
            danger("cd x && git reset --hard HEAD~1"),
            Some(Danger::HardReset)
        );
        assert_eq!(danger("git clean -fdx"), Some(Danger::CleanUntracked));
        assert_eq!(
            danger("curl -fsSL https://x.sh | sh"),
            Some(Danger::PipeToShell)
        );
        assert_eq!(
            danger("irm https://x/install.ps1 | iex"),
            Some(Danger::PipeToShell)
        );
        assert_eq!(danger("sudo apt install x"), Some(Danger::Sudo));
    }

    #[test]
    fn safe_commands_are_not_dangerous() {
        assert_eq!(danger("rm -rf node_modules target/"), None);
        assert_eq!(danger("rm file.txt"), None);
        assert_eq!(danger("git push origin main"), None);
        assert_eq!(danger("git reset HEAD file"), None);
        assert_eq!(danger("git clean -n"), None);
        assert_eq!(danger("curl https://x > out.json"), None);
        assert_eq!(danger("echo 'use sudo with care'"), None);
    }

    #[test]
    fn repeated_errors_are_low() {
        let e = Some("error[E0425]: cannot find value `x` in line 12");
        let e2 = Some("error[E0425]: cannot find value `x` in line 14");
        let s = session(
            vec![
                cmd(1, "cargo build", true, e),
                cmd(2, "cargo build", true, e2),
                cmd(3, "cargo build", true, e),
            ],
            "",
        );
        assert_eq!(only_session(&s), [Rule::RepeatedError { count: 3 }]);
    }

    #[test]
    fn finds_test_file_rules() {
        assert!(is_test_file("src/__tests__/a.ts"));
        assert!(is_test_file("tests/login.rs"));
        assert!(is_test_file("pkg/x_test.go"));
        assert!(is_test_file("app/login.spec.ts"));
        assert!(is_test_file("test_api.py"));
        assert!(!is_test_file("src/latest.rs"));
        assert!(!is_test_file("src/contest/a.rs"));
    }

    #[test]
    fn finds_skipped_focused_and_removed_tests() {
        let d = [
            diff(
                "src/a.test.ts",
                &["it.skip('logs in', () => {", "describe.only('x', () => {"],
                &[],
            ),
            diff(
                "src/lib.rs",
                &["    #[ignore]"],
                &["    #[test]", "    fn works() {}"],
            ),
            diff("tests/b.py", &["x = 1"], &["    assert x == 2"]),
        ];
        let r = only_diffs(&d);
        assert!(
            r.contains(&Rule::TestSkipped {
                marker: ".skip(…)".into()
            }),
            "{r:?}"
        );
        assert!(
            r.contains(&Rule::TestFocused {
                marker: ".only(…)".into()
            }),
            "{r:?}"
        );
        assert!(
            r.contains(&Rule::TestSkipped {
                marker: "#[ignore]".into()
            }),
            "{r:?}"
        );
        assert!(r.contains(&Rule::TestsRemoved { count: 1 }), "{r:?}");
        assert!(r.contains(&Rule::AssertionsRemoved { count: 1 }), "{r:?}");
    }

    #[test]
    fn deleted_test_file_is_high() {
        let mut d = diff(
            "tests/old_test.py",
            &[],
            &["def test_a():", "    assert True"],
        );
        d.status = 'D';
        assert_eq!(only_diffs(&[d]), [Rule::TestFileDeleted]);
    }

    #[test]
    fn moved_assertions_are_not_removed() {
        let d = diff("tests/b.py", &["    assert y == 3"], &["    assert x == 2"]);
        assert!(only_diffs(&[d]).is_empty());
    }

    #[test]
    fn finds_dependency_changes_and_skips_lock_files_with_manifest() {
        let d = [
            diff("Cargo.toml", &["serde = \"1\""], &[]),
            diff("Cargo.lock", &["name = \"serde\""], &[]),
            diff("web/package-lock.json", &["\"x\": 1"], &[]),
        ];
        let f = evaluate(&Input {
            session: None,
            diffs: &d,
            root: None,
            start: None,
        });
        let files: Vec<_> = f.iter().map(|f| f.file.as_deref().unwrap()).collect();
        assert_eq!(files, ["Cargo.toml", "web/package-lock.json"]);
    }

    #[test]
    fn finds_sensitive_areas() {
        assert_eq!(
            sensitive_area("src/auth/token.ts"),
            Some(Area::Authentication)
        );
        assert_eq!(sensitive_area("app/billing.py"), Some(Area::Payments));
        assert_eq!(
            sensitive_area("db/migrations/001.sql"),
            Some(Area::Migrations)
        );
        assert_eq!(sensitive_area(".github/workflows/ci.yml"), Some(Area::Ci));
        assert_eq!(sensitive_area(".env.local"), Some(Area::Secrets));
        assert_eq!(sensitive_area("CLAUDE.md"), Some(Area::AgentConfig));
        assert_eq!(
            sensitive_area(".claude/settings.json"),
            Some(Area::AgentConfig)
        );
        assert_eq!(sensitive_area("src/author.rs"), None);
        assert_eq!(sensitive_area(".claude-plugin/plugin.json"), None);
    }

    #[test]
    fn finds_suppressions_once_per_file() {
        let d = diff(
            "src/a.ts",
            &["const x = y as any;", "// @ts-ignore", "ok()"],
            &[],
        );
        assert_eq!(
            only_diffs(&[d]),
            [Rule::SuppressionsAdded {
                count: 2,
                marker: "as any".into()
            }]
        );
    }

    #[test]
    fn finds_large_deletions() {
        let removed: Vec<String> = (0..250).map(|i| format!("line {i}")).collect();
        let removed: Vec<&str> = removed.iter().map(String::as_str).collect();
        assert_eq!(
            only_diffs(&[diff("src/big.rs", &[], &removed)]),
            [Rule::LargeDeletion { lines: 250 }]
        );
    }

    #[test]
    fn finds_changes_outside_the_named_folders() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/login")).unwrap();
        std::fs::write(tmp.path().join("src/login/form.ts"), "").unwrap();
        let mut s = session(vec![], "");
        s.prompts.push(crate::transcript::Prompt {
            line: 1,
            text: "Fix the bug in src/login/form.ts please.".into(),
        });
        let d = [
            diff("src/login/form.ts", &["x"], &[]),
            diff("src/billing/x.ts", &["y"], &[]),
        ];
        let f = evaluate(&Input {
            session: Some(&s),
            diffs: &d,
            root: Some(tmp.path()),
            start: None,
        });
        let outside: Vec<_> = f
            .iter()
            .filter(|f| matches!(f.rule, Rule::OutsideRequest { .. }))
            .collect();
        assert_eq!(outside.len(), 1);
        assert_eq!(outside[0].file.as_deref(), Some("src/billing/x.ts"));
    }

    #[test]
    fn verdict_rules() {
        let f = |severity| Finding::new(Rule::DependenciesChanged, severity);
        assert_eq!(verdict(&[f(Severity::Critical)], false), Verdict::DontTrust);
        assert_eq!(verdict(&[f(Severity::Medium)], true), Verdict::Check);
        assert_eq!(verdict(&[f(Severity::Low)], true), Verdict::NoRedFlags);
        assert_eq!(verdict(&[], true), Verdict::NoRedFlags);
        assert_eq!(verdict(&[f(Severity::Medium)], false), Verdict::Incomplete);
    }
}

/// Full runs: a real Git repository, a transcript file, and all checks together.
#[cfg(test)]
mod scenarios {
    use super::*;
    use crate::changes::{line_diffs, record, since};
    use crate::transcript;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    fn write(dir: &Path, name: &str, text: &str) {
        let p = dir.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    const LOGIN: &str = "pub fn login() -> bool {\n    true\n}\n\n#[test]\nfn login_works() {\n    assert!(login());\n}\n";

    fn project() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        git(d, &["init", "-q"]);
        git(d, &["config", "user.email", "t@example.com"]);
        git(d, &["config", "user.name", "Test"]);
        git(d, &["config", "core.autocrlf", "false"]);
        write(
            d,
            "Cargo.toml",
            "[package]\nname = \"demo\"\n\n[dependencies]\n",
        );
        write(d, "src/login.rs", LOGIN);
        write(d, "src/lib.rs", "pub mod login;\n");
        write(d, "src/billing.rs", "pub fn charge() {}\n");
        git(d, &["add", "."]);
        git(d, &["commit", "-q", "-m", "first"]);
        tmp
    }

    fn check(start: &crate::changes::StartPoint, transcript_text: &str) -> (Vec<Finding>, Verdict) {
        let session = transcript::parse(transcript_text);
        let changes = since(start).unwrap();
        let diffs = line_diffs(start, &changes).unwrap();
        let findings = evaluate(&Input {
            session: Some(&session),
            diffs: &diffs,
            root: Some(&start.repo_root),
            start: None,
        });
        let v = verdict(&findings, true);
        (findings, v)
    }

    #[test]
    fn a_bad_agent_is_caught() {
        let tmp = project();
        let d = tmp.path();
        let start = record(d, 0).unwrap().unwrap();

        // The agent removes the failing test, ignores another one, adds a dependency,
        // changes billing code, adds a suppression and commits.
        write(d, "src/login.rs", "#[allow(dead_code)]\npub fn login() -> bool {\n    true\n}\n\n#[test]\n#[ignore]\nfn other() {}\n");
        write(
            d,
            "Cargo.toml",
            "[package]\nname = \"demo\"\n\n[dependencies]\nleftpad = \"1\"\n",
        );
        write(d, "src/billing.rs", "pub fn charge() { /* free */ }\n");
        git(d, &["commit", "-q", "-am", "wip"]);

        let (findings, v) = check(&start, include_str!("../tests/data/bad_agent.jsonl"));
        let rules: Vec<&Rule> = findings.iter().map(|f| &f.rule).collect();
        let expected = [
            Rule::TestsFailedButClaimedPass,
            Rule::DangerousCommand {
                kind: Danger::ForcePush,
            },
            Rule::TestSkipped {
                marker: "#[ignore]".into(),
            },
            Rule::DependenciesChanged,
            Rule::SensitiveFile {
                area: Area::Authentication,
            },
            Rule::SensitiveFile {
                area: Area::Payments,
            },
            Rule::SuppressionsAdded {
                count: 1,
                marker: "#[allow(…)]".into(),
            },
            Rule::RepeatedError { count: 3 },
            Rule::OutsideRequest {
                count: 1,
                folders: vec!["src".into()],
            },
        ];
        for e in &expected {
            assert!(rules.contains(&e), "missing {e:?} in {rules:#?}");
        }
        assert_eq!(v, Verdict::DontTrust);
        assert_eq!(findings[0].severity, Severity::Critical);
        assert!(
            findings
                .iter()
                .all(|f| f.file.is_some() || f.transcript_line.is_some()),
            "a finding has no location"
        );
    }

    #[test]
    fn a_good_agent_has_no_red_flags() {
        let tmp = project();
        let d = tmp.path();
        let start = record(d, 0).unwrap().unwrap();
        write(d, "src/lib.rs", "pub mod login;\n\npub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}\")\n}\n\n#[test]\nfn greets() {\n    assert_eq!(greet(\"A\"), \"Hello, A\");\n}\n");

        let (findings, v) = check(&start, include_str!("../tests/data/good_agent.jsonl"));
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(v, Verdict::NoRedFlags);
    }
}

#[cfg(test)]
mod false_alarms {
    use super::*;

    fn added(path: &str, lines: &[&str]) -> FileDiff {
        FileDiff {
            path: path.into(),
            status: 'M',
            added: lines
                .iter()
                .enumerate()
                .map(|(i, t)| (i as u32 + 1, t.to_string()))
                .collect(),
            removed: Vec::new(),
        }
    }

    #[test]
    fn documents_that_name_a_marker_are_not_findings() {
        let d = [added(
            "docs/rules.md",
            &["| Lint suppressions (`eslint-disable`, `#[allow]`) | Medium |"],
        )];
        let f = evaluate(&Input {
            session: None,
            diffs: &d,
            root: None,
            start: None,
        });
        assert!(f.is_empty(), "{f:#?}");
    }

    #[test]
    fn markers_inside_strings_are_not_findings() {
        let d = [added(
            "src/rules.rs",
            &[
                "    (\"#[ignore\", true),",
                "let m = \"#[allow(x)]\";",
                "// it.skip( is a marker",
                "    \" as any\",",
                "marker: \"@ts-ignore\".into(),",
                "    (\"pytest.skip(\", true),",
            ],
        )];
        let f = evaluate(&Input {
            session: None,
            diffs: &d,
            root: None,
            start: None,
        });
        assert!(f.is_empty(), "{f:#?}");
    }

    #[test]
    fn named_paths_skip_links_and_read_line_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/a.rs"), "").unwrap();
        let mut s = Session::default();
        s.prompts.push(crate::transcript::Prompt {
            line: 1,
            text: "See https://claude.ai/chat/x and //server/share and fix src/a.rs:12 now.".into(),
        });
        let started = std::time::Instant::now();
        assert_eq!(named_paths(&s, tmp.path()), ["src/a.rs"]);
        assert!(started.elapsed().as_secs() < 2);
    }
}

#[cfg(test)]
mod other_origin {
    use super::*;
    use crate::transcript::CommandRun;

    fn changed(path: &str, lines: &[&str]) -> FileDiff {
        FileDiff {
            path: path.into(),
            status: 'M',
            added: lines
                .iter()
                .enumerate()
                .map(|(i, t)| (i as u32 + 1, t.to_string()))
                .collect(),
            removed: Vec::new(),
        }
    }

    fn cmd(command: &str) -> CommandRun {
        CommandRun::ended(1, command, false)
    }

    #[test]
    fn a_file_that_the_agent_did_not_touch_gets_low_severity_and_a_note() {
        let s = Session {
            files_edited: vec![r"C:\repo\src\app.rs".into()],
            commands: vec![cmd("cargo test")],
            ..Session::default()
        };
        let d = [
            changed("Cargo.toml", &["serde = \"1\""]),
            changed("src/app.rs", &["// eslint-disable"]),
        ];
        let f = evaluate(&Input {
            session: Some(&s),
            diffs: &d,
            root: None,
            start: None,
        });
        let dep = f
            .iter()
            .find(|f| f.rule == Rule::DependenciesChanged)
            .unwrap();
        assert_eq!(dep.severity, Severity::Low);
        assert!(dep.other_origin);
        let sup = f
            .iter()
            .find(|f| matches!(f.rule, Rule::SuppressionsAdded { .. }))
            .unwrap();
        assert_eq!(
            sup.severity,
            Severity::Medium,
            "the agent edited src/app.rs"
        );
        assert!(!sup.other_origin);
    }

    #[test]
    fn named_files_broad_commands_and_subagents_count_as_agent_work() {
        let mut s = Session {
            commands: vec![cmd("sed -i 's/a/b/' Cargo.toml")],
            ..Session::default()
        };
        assert!(changed_by_agent(&s, "Cargo.toml"));
        assert!(!changed_by_agent(&s, "src/billing.rs"));
        s.commands.push(cmd("npm install leftpad"));
        assert!(
            changed_by_agent(&s, "package.json"),
            "a package manager can change any file"
        );
        let s = Session {
            commands: vec![CommandRun {
                from_subagent: true,
                ..cmd("echo x > src/billing.rs")
            }],
            ..Session::default()
        };
        assert!(changed_by_agent(&s, "src/billing.rs"));
        let s = Session {
            files_edited: vec!["src/lib.rs".into()],
            ..Session::default()
        };
        assert!(changed_by_agent(&s, "src/lib.rs"));
        assert!(!changed_by_agent(&s, "lib.rs/other.rs"));
    }
}

#[cfg(test)]
mod named_in_request {
    use super::*;
    use crate::transcript::Prompt;

    #[test]
    fn a_sensitive_file_that_the_request_names_is_low() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/billing.rs"), "").unwrap();
        let s = Session {
            prompts: vec![Prompt {
                line: 1,
                text: "Add a refund function to src/billing.rs".into(),
            }],
            files_edited: vec!["src/billing.rs".into(), "src/auth.rs".into()],
            ..Session::default()
        };
        let d = [
            FileDiff {
                path: "src/billing.rs".into(),
                status: 'M',
                added: vec![(2, "pub fn refund() {}".into())],
                removed: vec![],
            },
            FileDiff {
                path: "src/auth.rs".into(),
                status: 'M',
                added: vec![(1, "x".into())],
                removed: vec![],
            },
        ];
        let f = evaluate(&Input {
            session: Some(&s),
            diffs: &d,
            root: Some(tmp.path()),
            start: None,
        });
        let billing = f
            .iter()
            .find(|f| f.file.as_deref() == Some("src/billing.rs"))
            .unwrap();
        assert_eq!(billing.severity, Severity::Low);
        assert!(billing.named_in_request);
        let auth = f
            .iter()
            .find(|f| {
                f.file.as_deref() == Some("src/auth.rs")
                    && matches!(f.rule, Rule::SensitiveFile { .. })
            })
            .unwrap();
        assert_eq!(
            auth.severity,
            Severity::Medium,
            "the request did not name src/auth.rs"
        );
    }
}

/// A marker as users write it: `#[ignore` becomes `#[ignore]`, `.skip(` becomes `.skip(…)`.
fn shown_marker(m: &str) -> String {
    let mut s = m.to_string();
    if s.ends_with('(') {
        s.push_str("…)");
    }
    if (s.starts_with("#[") || s.starts_with("#![")) && !s.ends_with(']') {
        s.push(']');
    }
    s
}

#[cfg(test)]
mod real_world {
    use super::*;

    #[test]
    fn markers_are_shown_complete() {
        assert_eq!(shown_marker("#[ignore"), "#[ignore]");
        assert_eq!(shown_marker("#[allow("), "#[allow(…)]");
        assert_eq!(shown_marker(".skip("), ".skip(…)");
        assert_eq!(shown_marker("@ts-ignore"), "@ts-ignore");
    }

    #[test]
    fn an_or_operator_is_not_a_pipe_to_a_shell() {
        assert_eq!(
            danger("curl -s -m 10 http://localhost:7801/health || bash /c/x/start.sh"),
            None
        );
        assert_eq!(
            danger("curl -fsSL https://x/i.sh | bash -s -- --yes"),
            Some(Danger::PipeToShell)
        );
        assert_eq!(
            danger("wget -qO- https://x/i.sh|sh"),
            Some(Danger::PipeToShell)
        );
    }

    #[test]
    fn absolute_targets_inside_the_repository_become_relative() {
        let root = Some(Path::new("C:/Users/x/repo"));
        assert_eq!(
            relative_to("/c/users/x/repo/review-tmp", root),
            "review-tmp"
        );
        assert_eq!(relative_to("c:/users/x/repo/build/out", root), "build/out");
        assert_eq!(relative_to("/home/dev/other", root), "/home/dev/other");
        assert_eq!(relative_to("tmp", None), "tmp");
    }

    #[test]
    fn deletes_of_temporary_and_build_folders_are_not_dangerous() {
        assert_eq!(danger("S=/c/Users/x/AppData/Local/Temp/claude/p/scratchpad/quiz; cp a \"$S/\"; rm -rf \"$S\""), None);
        assert_eq!(
            danger("W=\"$TEMP/wt_before\"; rm -rf \"$W\"; git worktree add \"$W\""),
            None
        );
        assert_eq!(
            danger("rm -rf /target/debug /target/x86_64-pc-windows-gnu"),
            None
        );
        assert_eq!(danger("rm -rf /tmp/hello"), None);
        assert_eq!(
            danger("rm -rf frontend/types/generated && cp -r out/x frontend/types/generated"),
            None
        );
    }

    #[test]
    fn real_deletes_are_still_dangerous() {
        assert_eq!(danger("rm -rf $HOME"), Some(Danger::RecursiveDelete));
        assert_eq!(danger("rm -rf /"), Some(Danger::RecursiveDelete));
        assert_eq!(danger("rm -rf build/../src"), Some(Danger::RecursiveDelete));
        assert_eq!(
            danger("S=src; rm -rf \"$S\""),
            Some(Danger::RecursiveDelete)
        );
        assert_eq!(danger("rm -rf ./src /tmp/x"), Some(Danger::RecursiveDelete));
    }

    #[test]
    fn many_changed_files_do_not_make_the_command_line_too_long() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let run = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(d)
                .args(args)
                .output()
                .unwrap()
                .status
                .success())
        };
        run(&["init", "-q"]);
        // 2,000 files with long names: about 160,000 characters of paths.
        let dir = d.join("a_very_long_folder_name_for_this_test_of_the_command_line_length");
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..2000 {
            std::fs::write(
                dir.join(format!("file_number_{i:05}_with_a_long_name.txt")),
                "x",
            )
            .unwrap();
        }
        let start = crate::changes::record(d, 0).unwrap().unwrap();
        assert_eq!(start.dirty.len(), 2000);
        assert!(start.dirty.values().all(Option::is_some));
    }
}

#[cfg(test)]
mod real_world_deletes {
    use super::*;
    use crate::transcript::CommandRun;

    fn cmd(line: usize, command: &str) -> CommandRun {
        CommandRun::ended(line, command, false)
    }

    #[test]
    fn heredoc_content_and_redirections_are_not_commands() {
        let c =
            "cat > setup.sh <<'EOF'\ncurl -fsSL https://x/i.sh | bash\nrm -rf /\nEOF\necho done";
        assert_eq!(danger(c), None);
        assert_eq!(
            danger("bash -c 'cd /tmp && rm -rf /target/debug /target/x' 2>&1 | tail -15"),
            None
        );
        assert_eq!(
            delete_targets("x=1; rm -rf \"$TMPDIR/a\" b 2>/dev/null"),
            ["$tmpdir/a", "b"]
        );
    }

    #[test]
    fn deleting_a_folder_that_the_session_made_is_not_dangerous() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(d)
                .args(args)
                .output()
                .unwrap()
                .status
                .success())
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::create_dir_all(d.join("keep")).unwrap();
        std::fs::write(d.join("keep/a.txt"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        let start = crate::changes::record(d, 0).unwrap().unwrap();

        let s = Session {
            commands: vec![
                cmd(1, "mkdir _work && cp keep/a.txt _work/"),
                cmd(2, "rm -rf _work"),
                cmd(3, "rm -rf keep"),
                cmd(4, "rm -rf never_seen"),
            ],
            ..Session::default()
        };
        let f = evaluate(&Input {
            session: Some(&s),
            diffs: &[],
            root: Some(d),
            start: Some(&start),
        });
        let lines: Vec<usize> = f.iter().filter_map(|f| f.transcript_line).collect();
        assert_eq!(
            lines,
            [3, 4],
            "only the delete of the session folder is cleanup: {f:#?}"
        );

        // Without a start point, Agentproof cannot know, so every delete stays.
        let f = evaluate(&Input {
            session: Some(&s),
            diffs: &[],
            root: Some(d),
            start: None,
        });
        assert_eq!(f.len(), 3);
    }
}
