//! Put the transcript and the changes together, and save the result.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::changes::{self, Changes, Origin, StartPoint};
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
    pub session: Option<Session>,
    pub changes: Option<Changes>,
    /// Rule checks come in milestone 2. Until then there is no verdict.
    pub verdict: &'static str,
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

    let fingerprint = changes
        .as_ref()
        .map_or_else(|| "no-git".to_string(), |c| c.fingerprint.clone());
    let last: LastCheck = store::load(&dir.join("last.json"))?.unwrap_or_default();
    let session_files = changes.as_ref().map_or(0, |c| c.session_files().count());
    store::save(
        &dir.join("last.json"),
        &LastCheck {
            fingerprint: fingerprint.clone(),
        },
    )?;
    if !force && (last.fingerprint == fingerprint || (changes.is_some() && session_files == 0)) {
        return Ok(None);
    }

    let data_path = dir.join("check.json");
    let (added, deleted) = changes.as_ref().map_or((0, 0), Changes::session_lines);
    let message = messages::summary(&messages::Summary {
        files: session_files,
        added,
        deleted,
        commands: session.as_ref().map_or(0, |s| s.commands.len()),
        failed: session.as_ref().map_or(0, Session::failed_commands),
        before_session: changes.as_ref().map_or(0, |c| {
            c.files
                .iter()
                .filter(|f| f.origin == Origin::BeforeSession)
                .count()
        }),
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
        session,
        changes,
        verdict: "not_available",
        notes,
        message,
    };
    store::save(&data_path, &result)?;
    Ok(Some(result))
}
