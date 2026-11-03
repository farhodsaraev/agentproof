//! Where Agentproof keeps its own files. Nothing is written inside the repository.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

/// `AGENTPROOF_HOME`, else `~/.agentproof`.
pub fn data_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("AGENTPROOF_HOME") {
        return Ok(PathBuf::from(dir));
    }
    Ok(home()?.join(".agentproof"))
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("cannot find the home folder")
}

/// `CLAUDE_CONFIG_DIR`, else `~/.claude`.
fn claude_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(home()?.join(".claude"))
}

/// The folder name that Claude Code uses for a project: each character that is
/// not a letter or a digit becomes `-`.
pub fn project_key(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

pub fn session_dir(project: &Path, session: &str) -> Result<PathBuf> {
    if session.is_empty()
        || !session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("the session id {session:?} is not valid");
    }
    Ok(data_dir()?.join(project_key(project)).join(session))
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("cannot write {}", path.display()))
}

pub fn load<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(raw) => Ok(Some(serde_json::from_slice(&raw)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

pub fn transcript_for(project: &Path, session: &str) -> Result<PathBuf> {
    Ok(claude_dir()?
        .join("projects")
        .join(project_key(project))
        .join(format!("{session}.jsonl")))
}

/// The newest transcript of a project folder: (session id, path).
pub fn latest_transcript(project: &Path) -> Result<(String, PathBuf)> {
    let dir = claude_dir()?.join("projects").join(project_key(project));
    let entries = std::fs::read_dir(&dir)
        .with_context(|| format!("cannot find Claude Code sessions in {}", dir.display()))?;
    let newest = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(modified, _)| *modified);
    let Some((_, path)) = newest else {
        bail!("there are no Claude Code sessions in {}", dir.display());
    };
    let id = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok((id, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_key_matches_claude_code() {
        let key = project_key(Path::new(r"C:\Users\dilno\Desktop\Projects\openSource"));
        assert_eq!(key, "C--Users-dilno-Desktop-Projects-openSource");
        assert_eq!(project_key(Path::new("/home/a/my_repo")), "-home-a-my-repo");
    }

    #[test]
    fn rejects_unsafe_session_ids() {
        assert!(session_dir(Path::new("/r"), "../x").is_err());
        assert!(session_dir(Path::new("/r"), "").is_err());
    }
}
