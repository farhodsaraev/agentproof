//! Put the transcript and the changes together, and save the result.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use std::process::Stdio;

use crate::ai::{self, AiResult, ClaimVerdict};
use crate::changes::{self, Changes, StartPoint};
use crate::checks::{self, Finding, Verdict};
use crate::config::{self, AiConfig, Config};
use crate::hook::HookInput;
use crate::messages;
use crate::redact;
use crate::report;
use crate::store;
use crate::time::now_secs;
use crate::transcript::{self, Session};

/// The file that the session-start hook writes.
#[derive(Serialize, Deserialize)]
struct StartRecord {
    session_id: String,
    /// None if the folder is not in a Git repository.
    git: Option<StartPoint>,
}

#[derive(Default, Serialize, Deserialize)]
struct LastCheck {
    /// Keys of the findings that a message already showed.
    #[serde(default)]
    shown: Vec<String>,
    #[serde(default)]
    verdict: Option<Verdict>,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub agentproof_version: &'static str,
    pub session_id: String,
    pub project: PathBuf,
    pub transcript_path: Option<PathBuf>,
    pub checked_at: i64,
    pub start_point_found: bool,
    pub start_point: Option<StartPoint>,
    pub session_duration_secs: Option<i64>,
    pub failed_commands: usize,
    /// Lines of the session files only.
    pub lines_added_and_deleted: (u64, u64),
    pub session: Option<Session>,
    pub changes: Option<Changes>,
    pub verdict: Verdict,
    /// Why the engine cannot trust the transcript or the changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<&'static str>,
    /// The AI check found evidence for each claim. It is an opinion, so the best
    /// verdict stays "no red flags found".
    pub ai_confirms: bool,
    pub findings: Vec<Finding>,
    pub ai: AiStatus,
    pub strict: bool,
    pub notes: Vec<String>,
    pub report_path: PathBuf,
    /// None when there is nothing new to tell the user.
    pub message: Option<String>,
    /// Strict mode: the reason to stop the agent.
    pub block_reason: Option<String>,
}

/// Record the start point. An existing record is kept, because Claude Code
/// also runs the session-start hook when a session resumes.
pub fn record_start(cwd: &Path, session: &str) -> Result<()> {
    let path = store::session_dir(cwd, session)?.join("start.json");
    if path.exists() {
        return Ok(());
    }
    let record = StartRecord {
        session_id: session.to_string(),
        git: changes::record(cwd, now_secs())?,
    };
    store::save(&path, &record)
}

/// How a check runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// A manual check: always a message, with all findings.
    pub force: bool,
    /// Called by the stop hook: strict mode can stop the agent.
    pub hook: bool,
    /// Run the AI check now and wait for it. Else it runs in the background.
    pub ai_now: bool,
    /// The background AI check: no message, and the record of shown findings stays.
    pub background: bool,
}

/// The state of the AI check for this result.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AiStatus {
    /// The user settings do not turn the AI check on.
    Off,
    /// There is nothing to compare: no final message or no changes.
    NotNeeded,
    /// The check runs in the background.
    Running,
    Done(AiResult),
}

/// Check the session, and write the JSON file and the report.
pub fn run(input: &HookInput, opts: Options) -> Result<CheckResult> {
    let cwd = input.cwd.clone().unwrap_or_default();
    let (session_id, transcript_path) = match (&input.session_id, &input.transcript_path) {
        (Some(s), Some(t)) => (s.clone(), Some(t.clone())),
        (Some(s), None) => (s.clone(), Some(store::transcript_for(&cwd, s)?)),
        (None, _) => {
            let (s, t) = store::latest_transcript(&cwd)?;
            (s, Some(input.transcript_path.clone().unwrap_or(t)))
        }
    };
    let dir = store::session_dir(&cwd, &session_id)?;
    let mut notes = Vec::new();

    let mut session = match transcript_path.as_deref().map(transcript::read) {
        Some(Ok(s)) => Some(s),
        Some(Err(e)) => {
            notes.push(format!("{e:#}"));
            None
        }
        None => None,
    };

    let record: Option<StartRecord> = store::load(&dir.join("start.json"))?;
    let start_point_found = record.is_some();
    let start = match record {
        Some(r) => r.git,
        None => {
            notes.push("no start point was recorded; the base is the current commit".into());
            // Fall back to the last commit. Files that are dirty now count as session work.
            changes::record_base(&cwd, now_secs())?.map(|mut s| {
                s.dirty.clear();
                s
            })
        }
    };
    let changes = start.as_ref().map(changes::since).transpose()?;
    let mut diffs = match (&start, &changes) {
        (Some(s), Some(c)) => changes::line_diffs(s, c)?,
        _ => Vec::new(),
    };
    let session_files = changes.as_ref().map_or(0, |c| c.session_files().count());

    let cfg = config::load(start.as_ref().map(|s| s.repo_root.as_path())).unwrap_or_else(|e| {
        notes.push(format!("{e:#}"));
        Config::default()
    });

    let mut findings = checks::evaluate(&checks::Input {
        session: session.as_ref(),
        diffs: &diffs,
        root: start.as_ref().map(|s| s.repo_root.as_path()),
        start: if start_point_found {
            start.as_ref()
        } else {
            None
        },
    });

    // The AI check.
    let mut ai_status = match (&cfg.ai, &session, &changes) {
        (Some(ai_cfg), Some(s), Some(c)) => {
            if s.final_message.is_none() || (session_files == 0 && !opts.force) {
                AiStatus::NotNeeded
            } else {
                let key = ai_key(c, s);
                let saved = store::load::<AiResult>(&dir.join("ai.json"))?.filter(|r| r.key == key);
                match saved {
                    Some(r) => AiStatus::Done(r),
                    None if opts.ai_now || cfg.strict => {
                        let r = run_ai(ai_cfg, s, &diffs, key);
                        store::save(&dir.join("ai.json"), &r)?;
                        AiStatus::Done(r)
                    }
                    None => {
                        start_background(&dir, &session_id, transcript_path.as_deref(), &cwd)?;
                        AiStatus::Running
                    }
                }
            }
        }
        _ => AiStatus::Off,
    };
    let mut ai_confirms = false;
    if let (AiStatus::Done(r), Some(s)) = (&mut ai_status, &session) {
        if let Some(e) = &r.error {
            notes.push(format!("AI check: {e}"));
        }
        // An opinion counts only with evidence that the engine can find.
        let root = start.as_ref().map(|s| s.repo_root.as_path());
        for c in &mut r.claims {
            c.evidence_found = ai::evidence_found(&c.evidence, s, &diffs, root);
        }
        ai_confirms = r.error.is_none() && !r.claims.is_empty();
        for c in &r.claims {
            let (rule, severity) = match c.verdict {
                ClaimVerdict::Supported if c.evidence_found => continue,
                ClaimVerdict::Supported => {
                    ai_confirms = false;
                    continue;
                }
                // A contradiction with evidence that the engine cannot find is weaker.
                ClaimVerdict::Contradicted => (
                    checks::Rule::AiContradicted {
                        claim: c.claim.clone(),
                        reason: if c.evidence_found {
                            c.reason.clone()
                        } else {
                            format!("{} {}", c.reason, messages::EVIDENCE_NOT_FOUND)
                        },
                    },
                    if c.evidence_found {
                        checks::Severity::High
                    } else {
                        checks::Severity::Medium
                    },
                ),
                ClaimVerdict::Unsupported => (
                    checks::Rule::AiUnsupported {
                        claim: c.claim.clone(),
                        reason: c.reason.clone(),
                    },
                    checks::Severity::Medium,
                ),
            };
            ai_confirms = false;
            findings.push(Finding {
                transcript_line: s.final_message_line,
                evidence: c.evidence.clone(),
                ..Finding::new(rule, severity)
            });
        }
        findings.sort_by_key(|f| f.severity);
    }

    for f in &mut findings {
        f.text = messages::finding_text(&f.rule);
    }
    let unhealthy = session.as_ref().and_then(|s| health(s, session_files));
    let complete = session.is_some() && changes.is_some() && unhealthy.is_none();
    let verdict = checks::verdict(&findings, complete);
    let incomplete_reason = unhealthy.map(messages::unhealthy_text);

    // The checks are done. The message, the report and the saved files must not
    // show a secret, so remove the secrets from the texts now.
    for f in &mut findings {
        f.evidence = redact::redact(&f.evidence).0;
        f.text = redact::redact(&f.text).0;
    }
    if let Some(s) = &mut session {
        redact::session(s);
    }
    for d in &mut diffs {
        let secret = redact::secret_file(&d.path);
        for (_, line) in d.added.iter_mut().chain(d.removed.iter_mut()) {
            *line = if secret {
                redact::MARK.to_string()
            } else {
                redact::redact(line).0
            };
        }
    }
    for n in &mut notes {
        *n = redact::redact(n).0;
    }

    let data_path = dir.join("check.json");
    let report_path = dir.join("report.html");
    let last: LastCheck = store::load(&dir.join("last.json"))?.unwrap_or_default();

    // The hook message shows only findings that an earlier message did not show.
    let keys: Vec<String> = findings.iter().map(finding_key).collect();
    let new: Vec<&Finding> = findings
        .iter()
        .zip(&keys)
        .filter(|(_, k)| !last.shown.contains(k))
        .map(|(f, _)| f)
        .collect();
    let first = last.verdict.is_none();
    let quiet = opts.background
        || if first {
            changes.is_some() && session.is_some() && session_files == 0 && findings.is_empty()
        } else {
            new.is_empty() && last.verdict == Some(verdict)
        };

    // Strict mode stops the agent once for each new critical finding.
    let new_critical: Vec<&Finding> = new
        .iter()
        .copied()
        .filter(|f| f.severity == checks::Severity::Critical)
        .collect();
    let block_reason = (cfg.strict && opts.hook && !new_critical.is_empty())
        .then(|| messages::block_reason(&new_critical));

    let all: Vec<&Finding> = findings.iter().collect();
    let message = (opts.force || !quiet).then(|| {
        messages::summary(&messages::Summary {
            verdict,
            findings: &findings,
            shown: if opts.force { &all } else { &new },
            only_new: !opts.force && !first,
            no_start: !start_point_found && (opts.force || first),
            no_git: changes.is_none(),
            no_transcript: session.is_none(),
            incomplete_reason,
            ai_running: matches!(ai_status, AiStatus::Running),
            ai_confirms,
            report_path: &report_path.to_string_lossy().replace('\\', "/"),
        })
    });

    if !opts.background {
        let mut shown = last.shown;
        for k in keys {
            if !shown.contains(&k) {
                shown.push(k);
            }
        }
        store::save(
            &dir.join("last.json"),
            &LastCheck {
                shown,
                verdict: Some(verdict),
            },
        )?;
    }

    let result = CheckResult {
        agentproof_version: env!("CARGO_PKG_VERSION"),
        session_id,
        project: cwd,
        transcript_path,
        checked_at: now_secs(),
        start_point_found,
        // The list of untracked paths can be long. It stays in start.json only.
        start_point: start.map(|mut s| {
            s.untracked = None;
            s
        }),
        session_duration_secs: session.as_ref().and_then(Session::duration_secs),
        failed_commands: session.as_ref().map_or(0, Session::failed_commands),
        lines_added_and_deleted: changes.as_ref().map_or((0, 0), Changes::session_lines),
        session,
        changes,
        verdict,
        incomplete_reason,
        ai_confirms,
        findings,
        ai: ai_status,
        strict: cfg.strict,
        notes,
        report_path: report_path.clone(),
        message,
        block_reason,
    };
    store::save(&data_path, &result)?;
    std::fs::write(&report_path, report::render(&result, &diffs))?;
    Ok(result)
}

/// The health check: a reason not to trust the transcript, if any. An unknown
/// format must not give "no red flags found". `session_files` is the number of
/// files that changed in the session.
fn health(s: &Session, session_files: usize) -> Option<messages::Unhealthy> {
    use messages::Unhealthy;
    let tool_calls: usize = s.tool_calls.values().sum();
    let known = s.prompts.len() + s.assistant_messages + tool_calls;
    if known == 0 && s.ignored_lines == 0 && s.bad_lines == 0 {
        Some(Unhealthy::Empty)
    } else if known == 0 {
        Some(Unhealthy::UnknownFormat)
    } else if s.bad_lines > known {
        Some(Unhealthy::MostLinesBad)
    } else if session_files > 0 && s.files_edited.is_empty() && s.commands.is_empty() {
        Some(Unhealthy::NoAgentWork)
    } else {
        None
    }
}

/// The input of the AI check. A new key means a new AI check.
fn ai_key(c: &Changes, s: &Session) -> String {
    let input = format!(
        "{}|{}|{}",
        c.fingerprint,
        s.final_message.as_deref().unwrap_or(""),
        s.commands.len()
    );
    changes::hash_hex(input.as_bytes())
}

fn run_ai(cfg: &AiConfig, s: &Session, diffs: &[changes::FileDiff], key: String) -> AiResult {
    let req = ai::build(s, diffs);
    let (claims, error) = match ai::call(cfg, &req) {
        Ok(c) => (c, None),
        Err(e) => (Vec::new(), Some(format!("{e:#}"))),
    };
    AiResult {
        key,
        model: cfg.model().unwrap_or_default(),
        claims,
        removed_secrets: req.removed_secrets,
        shortened_input: req.shortened,
        error,
        checked_at: now_secs(),
    }
}

/// The lock file stops a second background AI check for the same session.
const AI_LOCK: &str = "ai.lock";
const AI_LOCK_MAX_AGE_SECS: u64 = 600;

/// Start the AI check as a separate process, so that the agent does not wait.
fn start_background(
    dir: &Path,
    session: &str,
    transcript: Option<&Path>,
    cwd: &Path,
) -> Result<()> {
    let lock = dir.join(AI_LOCK);
    let running = std::fs::metadata(&lock)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age.as_secs() < AI_LOCK_MAX_AGE_SECS);
    if running {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&lock, now_secs().to_string())?;

    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    cmd.args(["ai", "--session", session, "--cwd"]).arg(cwd);
    if let Some(t) = transcript {
        cmd.arg("--transcript").arg(t);
    }
    // No shared input or output: Claude Code must not wait for this process.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn()?;
    Ok(())
}

/// The background process calls this when it is done.
pub fn end_background(input: &HookInput) {
    let cwd = input.cwd.clone().unwrap_or_default();
    if let Some(s) = &input.session_id {
        if let Ok(dir) = store::session_dir(&cwd, s) {
            let _ = std::fs::remove_file(dir.join(AI_LOCK));
        }
    }
}

/// Line numbers are not in the key, because they move when the agent edits a file.
/// Without secrets, because `last.json` keeps the keys without secrets.
fn finding_key(f: &Finding) -> String {
    let key = format!(
        "{}|{}|{}",
        serde_json::to_string(&f.rule).unwrap_or_default(),
        f.file.as_deref().unwrap_or(""),
        f.evidence
    );
    redact::redact(&key).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }

    const PROMPT: &str = r#"{"type":"user","timestamp":"2026-11-03T10:00:00.000Z","message":{"role":"user","content":"Update src/app.js <b>now</b>."}}"#;
    const PUSH: &str = r#"{"type":"assistant","timestamp":"2026-11-03T10:01:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"p1","name":"Bash","input":{"command":"git push --force"}}]}}"#;
    const DONE: &str = r#"{"type":"assistant","timestamp":"2026-11-03T10:02:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Done <script>alert(1)</script>"}]}}"#;
    const EDIT: &str = r#"{"type":"assistant","timestamp":"2026-11-03T10:00:30.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"e1","name":"Edit","input":{"file_path":"src/app.js","old_string":"a","new_string":"b"}}]}}"#;

    #[test]
    fn the_health_check_finds_transcripts_that_it_cannot_trust() {
        use messages::Unhealthy;
        let h = |text: &str, files| health(&transcript::parse(text), files);
        // Experiment e3b: an empty transcript.
        assert_eq!(h("", 0), Some(Unhealthy::Empty));
        // Experiment e3: lines in a format that the engine does not know.
        assert_eq!(
            h(
                "{\"event\":\"start\"}\n{\"event\":\"text\",\"body\":\"hi\"}",
                2
            ),
            Some(Unhealthy::UnknownFormat)
        );
        assert_eq!(
            h(&format!("not json\nnot json either\n{PROMPT}"), 0),
            Some(Unhealthy::MostLinesBad)
        );
        assert_eq!(
            h(&format!("{PROMPT}\n{DONE}"), 1),
            Some(Unhealthy::NoAgentWork)
        );
        assert_eq!(h(&format!("{PROMPT}\n{DONE}"), 0), None);
        assert_eq!(h(&format!("{PROMPT}\n{EDIT}\n{DONE}"), 1), None);
        assert_eq!(h(&format!("{PROMPT}\n{PUSH}\n{DONE}"), 1), None);
    }

    /// One test only, because it sets the data folder for the whole process.
    #[test]
    fn hook_messages_show_only_news_and_the_report_is_written() {
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("AGENTPROOF_HOME", home.path());
        let repo = tempfile::tempdir().unwrap();
        let d = repo.path();
        git(d, &["init", "-q"]);
        git(d, &["config", "user.email", "t@example.com"]);
        git(d, &["config", "user.name", "T"]);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/app.js"), "a\n").unwrap();
        git(d, &["add", "."]);
        git(d, &["commit", "-q", "-m", "first"]);
        let t = d.join("t.jsonl");
        let input = HookInput {
            session_id: Some("s1".into()),
            transcript_path: Some(t.clone()),
            cwd: Some(d.to_path_buf()),
        };

        std::fs::write(&t, format!("{PROMPT}\n")).unwrap();
        let hook = Options {
            hook: true,
            ..Options::default()
        };
        let manual = Options {
            force: true,
            ai_now: true,
            ..Options::default()
        };
        record_start(d, "s1").unwrap();
        let r = run(&input, hook).unwrap();
        assert!(r.message.is_none(), "no change and no finding: no message");

        std::fs::write(d.join("src/app.js"), "b\n").unwrap();
        std::fs::write(&t, format!("{PROMPT}\n{EDIT}\n{DONE}\n")).unwrap();
        let r = run(&input, hook).unwrap();
        let m = r.message.unwrap();
        assert!(m.starts_with("Agentproof: no red flags found."), "{m}");

        std::fs::write(d.join("src/app.js"), "c\n").unwrap();
        assert!(
            run(&input, hook).unwrap().message.is_none(),
            "same verdict, no new finding"
        );

        std::fs::write(&t, format!("{PROMPT}\n{PUSH}\n{DONE}\n")).unwrap();
        let m = run(&input, hook).unwrap().message.unwrap();
        assert!(
            m.contains(
                "New. Critical: The agent ran a dangerous command: force push (transcript line 2)."
            ),
            "{m}"
        );
        assert!(
            run(&input, hook).unwrap().message.is_none(),
            "the push is not news again"
        );
        assert!(
            run(&input, manual)
                .unwrap()
                .message
                .unwrap()
                .contains("force push"),
            "a manual check shows all"
        );

        let html = std::fs::read_to_string(
            home.path()
                .join(store::project_key(d))
                .join("s1/report.html"),
        )
        .unwrap();
        for part in [
            "Do not trust this work",
            "What to verify",
            "Red flags",
            "Claimed and done",
            "Blast radius",
            "What happened",
            "src/app.js",
        ] {
            assert!(html.contains(part), "missing {part}");
        }
        assert!(
            !html.contains("<script>"),
            "transcript text must be escaped"
        );
        assert!(html.contains("&lt;b&gt;now&lt;/b&gt;"));
        assert!(
            !html.contains("src=\"http") && !html.contains("href=\"http"),
            "no external requests"
        );
        assert!(html.contains("The AI check is off."));

        // Turn on the AI check with a fake Anthropic server.
        let url = fake_anthropic(2);
        std::env::set_var("AGENTPROOF_TEST_KEY_FLOW", "k");
        std::fs::write(
            home.path().join("config.toml"),
            format!("[ai]\nprovider = \"anthropic\"\nbase_url = \"{url}\"\napi_key_env = \"AGENTPROOF_TEST_KEY_FLOW\"\n"),
        )
        .unwrap();
        let r = run(&input, manual).unwrap();
        let m = r.message.unwrap();
        assert!(
            m.contains("AI opinion: the evidence does not agree with the claim \"Done\""),
            "{m}"
        );
        assert!(matches!(r.ai, AiStatus::Done(ref a) if a.error.is_none()));
        let html = std::fs::read_to_string(&r.report_path).unwrap();
        assert!(html.contains("Contradicted") && html.contains("opinion"));

        // Strict mode stops the agent for a new critical finding, and only once.
        std::fs::write(d.join(".agentproof.toml"), "strict = true\n").unwrap();
        let rm = PUSH
            .replace("git push --force", "rm -rf src")
            .replace("p1", "p2");
        std::fs::write(&t, format!("{PROMPT}\n{PUSH}\n{rm}\n{DONE}\n")).unwrap();
        let r = run(&input, hook).unwrap();
        let reason = r.block_reason.unwrap();
        assert!(reason.contains("recursive delete"), "{reason}");
        assert!(
            !reason.contains("force push"),
            "only new critical findings: {reason}"
        );
        assert!(run(&input, hook).unwrap().block_reason.is_none());

        // Experiment e6: fake keys in a prompt, a command, its error and a secret file.
        std::fs::remove_file(d.join(".agentproof.toml")).unwrap();
        std::fs::remove_file(home.path().join("config.toml")).unwrap();
        std::fs::create_dir_all(d.join("src/auth")).unwrap();
        std::fs::write(
            d.join("src/auth/secrets.json"),
            "{\"stripe\": \"sk_live_51HfileFAKE0123456789abcdef\"}\n",
        )
        .unwrap();
        let keys = PROMPT.replace(
            "Update src/app.js",
            "Use sk_live_51HpromptFAKE0123456789abcdef in src/app.js",
        );
        let curl = r#"{"type":"assistant","timestamp":"2026-11-03T10:01:30.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"Bash","input":{"command":"curl -H \"Authorization: Bearer sk_live_51HcmdFAKE0123456789abcdef\" https://api.stripe.com"}}]}}"#;
        let fail = r#"{"type":"user","timestamp":"2026-11-03T10:01:31.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","is_error":true,"content":"Exit code 1\nerror: invalid key sk_live_51HcmdFAKE0123456789abcdef"}]}}"#;
        std::fs::write(&t, format!("{keys}\n{EDIT}\n{curl}\n{fail}\n{DONE}\n")).unwrap();
        let r = run(&input, manual).unwrap();
        let dir = home.path().join(store::project_key(d)).join("s1");
        for file in ["check.json", "report.html", "last.json"] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap();
            assert!(!text.contains("sk_live_51H"), "a key is in {file}");
        }
        assert!(!r.message.as_deref().unwrap_or("").contains("sk_live_51H"));
        let json = serde_json::to_string(&store::clean_json(&r).unwrap()).unwrap();
        assert!(!json.contains("sk_live_51H"), "the JSON output has a key");
    }

    /// A fake Anthropic server for `n` requests. It always says that the claim is false.
    fn fake_anthropic(n: usize) -> String {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for _ in 0..n {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    if line == "\r\n" {
                        break;
                    }
                }
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                let claims = r#"{"claims":[{"claim":"Done","verdict":"contradicted","reason":"The change is not complete.","evidence":"src/app.js, line 1"}]}"#;
                let answer = serde_json::json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": claims}]}).to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                stream.write_all(resp.as_bytes()).unwrap();
            }
        });
        url
    }
}
