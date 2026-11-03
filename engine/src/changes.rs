//! Read the changes in the repository with `git`. This module only reads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// The Git id of an empty tree. Used as the base when the repository had no commit.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// The state of the repository when a session started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartPoint {
    pub recorded_at: i64,
    pub repo_root: PathBuf,
    /// None if the repository had no commit.
    pub head: Option<String>,
    /// Files that had changes before the session, with their content id.
    pub dirty: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Changed during the session.
    Session,
    /// Changed before the session, and not changed after.
    BeforeSession,
    /// Changed before the session, and changed again during it.
    BeforeAndSession,
}

#[derive(Debug, Serialize)]
pub struct FileChange {
    pub path: String,
    /// A = added, M = changed, D = deleted.
    pub status: char,
    /// None for binary files.
    pub added: Option<u64>,
    pub deleted: Option<u64>,
    pub origin: Origin,
}

#[derive(Debug, Serialize)]
pub struct Changes {
    pub base: String,
    pub files: Vec<FileChange>,
    /// A hash of the changes. The same value means that nothing changed.
    pub fingerprint: String,
}

impl Changes {
    pub fn session_files(&self) -> impl Iterator<Item = &FileChange> {
        self.files
            .iter()
            .filter(|f| f.origin != Origin::BeforeSession)
    }

    pub fn session_lines(&self) -> (u64, u64) {
        self.session_files().fold((0, 0), |(a, d), f| {
            (a + f.added.unwrap_or(0), d + f.deleted.unwrap_or(0))
        })
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("cannot start git; make sure that git is installed")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

fn git_text(dir: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&git(dir, args)?).trim().to_string())
}

/// None if the folder is not in a Git repository.
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    git_text(dir, &["rev-parse", "--show-toplevel"])
        .ok()
        .map(PathBuf::from)
}

fn head(root: &Path) -> Option<String> {
    git_text(root, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .ok()
        .filter(|s| !s.is_empty())
}

fn split_nul(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Content ids of files in the work tree. None for a file that does not exist.
fn content_ids(root: &Path, paths: &[String]) -> Result<BTreeMap<String, Option<String>>> {
    let existing: Vec<&String> = paths.iter().filter(|p| root.join(p).is_file()).collect();
    let mut ids = BTreeMap::new();
    if !existing.is_empty() {
        let mut args = vec!["hash-object", "--"];
        args.extend(existing.iter().map(|p| p.as_str()));
        let out = git_text(root, &args)?;
        for (path, id) in existing.iter().zip(out.lines()) {
            ids.insert((*path).clone(), Some(id.to_string()));
        }
    }
    for p in paths {
        ids.entry(p.clone()).or_insert(None);
    }
    Ok(ids)
}

fn dirty_paths(root: &Path) -> Result<Vec<String>> {
    let raw = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ],
    )?;
    Ok(split_nul(&raw)
        .into_iter()
        .filter_map(|e| e.get(3..).map(str::to_string))
        .collect())
}

pub fn record(dir: &Path, now: i64) -> Result<Option<StartPoint>> {
    let Some(root) = repo_root(dir) else {
        return Ok(None);
    };
    let dirty = content_ids(&root, &dirty_paths(&root)?)?;
    Ok(Some(StartPoint {
        recorded_at: now,
        head: head(&root),
        repo_root: root,
        dirty,
    }))
}

/// All changes from the start point to the current work tree, including commits.
pub fn since(start: &StartPoint) -> Result<Changes> {
    let root = &start.repo_root;
    let base = start.head.clone().unwrap_or_else(|| EMPTY_TREE.to_string());

    let mut files: BTreeMap<String, FileChange> = BTreeMap::new();
    let status = split_nul(&git(
        root,
        &["diff", "--no-renames", "--name-status", "-z", &base],
    )?);
    for pair in status.chunks(2) {
        if let [code, path] = pair {
            let status = code.chars().next().unwrap_or('M');
            files.insert(
                path.clone(),
                FileChange {
                    path: path.clone(),
                    status,
                    added: None,
                    deleted: None,
                    origin: Origin::Session,
                },
            );
        }
    }
    let numstat = split_nul(&git(
        root,
        &["diff", "--no-renames", "--numstat", "-z", &base],
    )?);
    for entry in numstat {
        let mut parts = entry.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if let Some(f) = files.get_mut(path) {
            f.added = a.parse().ok();
            f.deleted = d.parse().ok();
        }
    }
    let untracked = split_nul(&git(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    for path in &untracked {
        let added = std::fs::read(root.join(path))
            .ok()
            .filter(|b| !b[..b.len().min(8000)].contains(&0))
            .map(|b| {
                let n = b.iter().filter(|c| **c == b'\n').count() as u64;
                if b.last().is_some_and(|c| *c != b'\n') {
                    n + 1
                } else {
                    n
                }
            });
        files.insert(
            path.clone(),
            FileChange {
                path: path.clone(),
                status: 'A',
                added,
                deleted: added.map(|_| 0),
                origin: Origin::Session,
            },
        );
    }

    // Find which changes were already there before the session.
    let dirty_now: Vec<String> = files
        .keys()
        .filter(|p| start.dirty.contains_key(*p))
        .cloned()
        .collect();
    let ids_now = content_ids(root, &dirty_now)?;
    for (path, id_now) in &ids_now {
        if let Some(f) = files.get_mut(path) {
            f.origin = if start.dirty.get(path) == Some(id_now) {
                Origin::BeforeSession
            } else {
                Origin::BeforeAndSession
            };
        }
    }

    // The fingerprint covers the full change text and the untracked file contents.
    let mut h = Fnv::new();
    h.write(base.as_bytes());
    h.write(&git(root, &["diff", "--no-renames", "--binary", &base])?);
    for (path, id) in content_ids(root, &untracked)? {
        h.write(path.as_bytes());
        h.write(id.unwrap_or_default().as_bytes());
    }

    Ok(Changes {
        base,
        files: files.into_values().collect(),
        fingerprint: format!("{:016x}", h.0),
    })
}

/// FNV-1a 64-bit hash. Stable across Rust versions, unlike the std hasher.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap();
    }

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
    }

    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        run(d, &["init", "-q"]);
        run(d, &["config", "user.email", "t@example.com"]);
        run(d, &["config", "user.name", "Test"]);
        run(d, &["config", "core.autocrlf", "false"]);
        tmp
    }

    #[test]
    fn counts_agent_commits_and_keeps_earlier_changes_apart() {
        let tmp = repo();
        let d = tmp.path();
        for f in ["a.txt", "b.txt", "keep.txt"] {
            write(d, f, "one\n");
        }
        run(d, &["add", "."]);
        run(d, &["commit", "-q", "-m", "first"]);

        // The user changes two files before the session.
        write(d, "b.txt", "one\nuser\n");
        write(d, "keep.txt", "one\nuser\n");
        let start = record(d, 0).unwrap().unwrap();
        assert_eq!(start.dirty.len(), 2);

        // The agent commits a change, adds a file and changes b.txt again.
        write(d, "a.txt", "one\ntwo\nthree\n");
        run(d, &["commit", "-q", "-am", "agent"]);
        write(d, "new.txt", "x\ny");
        write(d, "b.txt", "one\nuser\nagent\n");

        let c = since(&start).unwrap();
        let by: BTreeMap<_, _> = c.files.iter().map(|f| (f.path.as_str(), f)).collect();
        assert_eq!(by["a.txt"].origin, Origin::Session);
        assert_eq!((by["a.txt"].added, by["a.txt"].deleted), (Some(2), Some(0)));
        assert_eq!(by["new.txt"].status, 'A');
        assert_eq!(by["new.txt"].added, Some(2));
        assert_eq!(by["b.txt"].origin, Origin::BeforeAndSession);
        assert_eq!(by["keep.txt"].origin, Origin::BeforeSession);
        assert_eq!(c.session_files().count(), 3);
    }

    #[test]
    fn works_without_a_commit_and_detects_new_changes() {
        let tmp = repo();
        let d = tmp.path();
        let start = record(d, 0).unwrap().unwrap();
        assert!(start.head.is_none());
        write(d, "a.txt", "1\n");
        let first = since(&start).unwrap();
        assert_eq!(first.files.len(), 1);
        assert_eq!(since(&start).unwrap().fingerprint, first.fingerprint);
        write(d, "a.txt", "2\n");
        assert_ne!(since(&start).unwrap().fingerprint, first.fingerprint);
    }

    #[test]
    fn a_folder_outside_git_has_no_start_point() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(record(tmp.path(), 0).unwrap().is_none());
    }
}
