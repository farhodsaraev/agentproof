//! Put the transcript and the changes together, and save the result.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::changes::{self, Changes, StartPoint};
use crate::checks::{self, Finding, Verdict};
use crate::hook::HookInput;
use crate::messages;
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
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
    pub report_path: PathBuf,
    /// None when there is nothing new to tell the user.
    pub message: Option<String>,
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

/// Check the session, and write the JSON file and the report. `force` (a manual
/// check) always gives a message; the hook gives one only when there is news.
pub fn run(input: &HookInput, force: bool) -> Result<CheckResult> {
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

    let session = match transcript_path.as_deref().map(transcript::read) {
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
            changes::record(&cwd, now_secs())?.map(|mut s| {
                s.dirty.clear();
                s
            })
        }
    };
    let changes = start.as_ref().map(changes::since).transpose()?;
    let diffs = match (&start, &changes) {
        (Some(s), Some(c)) => changes::line_diffs(s, c)?,
        _ => Vec::new(),
    };

    let mut findings = checks::evaluate(&checks::Input {
        session: session.as_ref(),
        diffs: &diffs,
        root: start.as_ref().map(|s| s.repo_root.as_path()),
    });
    for f in &mut findings {
        f.text = messages::finding_text(&f.rule);
    }
    let complete = session.is_some() && changes.is_some();
    let verdict = checks::verdict(&findings, complete);

    let data_path = dir.join("check.json");
    let report_path = dir.join("report.html");
    let last: LastCheck = store::load(&dir.join("last.json"))?.unwrap_or_default();
    let session_files = changes.as_ref().map_or(0, |c| c.session_files().count());

    // The hook message shows only findings that an earlier message did not show.
    let keys: Vec<String> = findings.iter().map(finding_key).collect();
    let new: Vec<&Finding> = findings
        .iter()
        .zip(&keys)
        .filter(|(_, k)| !last.shown.contains(k))
        .map(|(f, _)| f)
        .collect();
    let first = last.verdict.is_none();
    let quiet = if first {
        changes.is_some() && session_files == 0 && findings.is_empty()
    } else {
        new.is_empty() && last.verdict == Some(verdict)
    };
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

    let all: Vec<&Finding> = findings.iter().collect();
    let message = (force || !quiet).then(|| {
        messages::summary(&messages::Summary {
            verdict,
            findings: &findings,
            shown: if force { &all } else { &new },
            only_new: !force && !first,
            no_start: !start_point_found && (force || first),
            no_git: changes.is_none(),
            no_transcript: session.is_none(),
            report_path: &report_path.to_string_lossy().replace('\\', "/"),
        })
    });

    let result = CheckResult {
        agentproof_version: env!("CARGO_PKG_VERSION"),
        session_id,
        project: cwd,
        transcript_path,
        checked_at: now_secs(),
        start_point_found,
        start_point: start,
        session_duration_secs: session.as_ref().and_then(Session::duration_secs),
        failed_commands: session.as_ref().map_or(0, Session::failed_commands),
        lines_added_and_deleted: changes.as_ref().map_or((0, 0), Changes::session_lines),
        session,
        changes,
        verdict,
        findings,
        notes,
        report_path: report_path.clone(),
        message,
    };
    store::save(&data_path, &result)?;
    std::fs::write(&report_path, report::render(&result, &diffs))?;
    Ok(result)
}

/// Line numbers are not in the key, because they move when the agent edits a file.
fn finding_key(f: &Finding) -> String {
    format!(
        "{}|{}|{}",
        serde_json::to_string(&f.rule).unwrap_or_default(),
        f.file.as_deref().unwrap_or(""),
        f.evidence
    )
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
        record_start(d, "s1").unwrap();
        let r = run(&input, false).unwrap();
        assert!(r.message.is_none(), "no change and no finding: no message");

        std::fs::write(d.join("src/app.js"), "b\n").unwrap();
        std::fs::write(&t, format!("{PROMPT}\n{DONE}\n")).unwrap();
        let r = run(&input, false).unwrap();
        let m = r.message.unwrap();
        assert!(m.starts_with("Agentproof: no red flags found."), "{m}");

        std::fs::write(d.join("src/app.js"), "c\n").unwrap();
        assert!(
            run(&input, false).unwrap().message.is_none(),
            "same verdict, no new finding"
        );

        std::fs::write(&t, format!("{PROMPT}\n{PUSH}\n{DONE}\n")).unwrap();
        let m = run(&input, false).unwrap().message.unwrap();
        assert!(
            m.contains(
                "New. Critical: The agent ran a dangerous command: force push (transcript line 2)."
            ),
            "{m}"
        );
        assert!(
            run(&input, false).unwrap().message.is_none(),
            "the push is not news again"
        );
        assert!(
            run(&input, true)
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
    }
}
