//! Put the transcript and the changes together, and save the result.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::changes::{self, Changes, StartPoint};
use crate::checks::{self, Finding, Verdict};
use crate::hook::HookInput;
use crate::messages;
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
    fingerprint: String,
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
    pub message: String,
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

/// Returns None when nothing changed since the last check and `force` is false.
pub fn run(input: &HookInput, force: bool) -> Result<Option<CheckResult>> {
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

    // A new message only when the changes or the findings are new.
    let fingerprint = format!(
        "{}:{}",
        changes
            .as_ref()
            .map_or("no-git", |c| c.fingerprint.as_str()),
        changes::hash_hex(serde_json::to_string(&findings)?.as_bytes())
    );
    let last: LastCheck = store::load(&dir.join("last.json"))?.unwrap_or_default();
    let session_files = changes.as_ref().map_or(0, |c| c.session_files().count());
    store::save(
        &dir.join("last.json"),
        &LastCheck {
            fingerprint: fingerprint.clone(),
        },
    )?;
    let nothing_to_say = changes.is_some() && session_files == 0 && findings.is_empty();
    if !force && (last.fingerprint == fingerprint || nothing_to_say) {
        return Ok(None);
    }

    let data_path = dir.join("check.json");
    let message = messages::summary(&messages::Summary {
        verdict,
        findings: &findings,
        files: session_files,
        commands: session.as_ref().map_or(0, |s| s.commands.len()),
        no_start: !start_point_found,
        no_git: changes.is_none(),
        no_transcript: session.is_none(),
        data_path: &data_path.to_string_lossy().replace('\\', "/"),
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
        message,
    };
    store::save(&data_path, &result)?;
    Ok(Some(result))
}
