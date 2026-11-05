//! Read the changes in the repository with `git`. This module only reads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
    /// Paths that git did not track at the start, ignored paths too. A folder
    /// with no tracked file gives one line that ends with `/`, for example
    /// `data/`. None in a record of an older version, or when the list was not made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub untracked: Option<Vec<String>>,
    /// The list stopped at the time limit or the count limit, so it can miss paths.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub untracked_partial: bool,
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
    git_input(dir, args, None)
}

/// Run git. `input` goes to standard input, so that a long list of paths does
/// not make the command line too long (Windows allows about 32,000 characters).
fn git_input(dir: &Path, args: &[&str], input: Option<String>) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("cannot start git; make sure that git is installed")
        } else {
            anyhow::anyhow!("cannot start git: {e}")
        }
    })?;
    // Write on a separate thread, so that a full output pipe cannot block the input.
    let writer = match (input, child.stdin.take()) {
        (Some(text), Some(mut stdin)) => Some(std::thread::spawn(move || {
            use std::io::Write;
            stdin.write_all(text.as_bytes())
        })),
        _ => None,
    };
    let out = child.wait_with_output().context("git did not finish")?;
    if let Some(w) = writer {
        let _ = w.join();
    }
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
        let list: String = existing.iter().map(|p| format!("{p}\n")).collect();
        let out = String::from_utf8_lossy(&git_input(
            root,
            &["hash-object", "--stdin-paths"],
            Some(list),
        )?)
        .into_owned();
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

/// The time limit and the count limit for the list of untracked paths at the start.
const UNTRACKED_TIME: Duration = Duration::from_millis(300);
const UNTRACKED_MAX: usize = 50_000;

/// The start point of a session, with the list of untracked and ignored paths.
pub fn record(dir: &Path, now: i64) -> Result<Option<StartPoint>> {
    record_with(dir, now, true)
}

/// The start point without the list of untracked paths. A check with no recorded
/// start point uses this: the paths that exist now do not show the start.
pub fn record_base(dir: &Path, now: i64) -> Result<Option<StartPoint>> {
    record_with(dir, now, false)
}

fn record_with(dir: &Path, now: i64, list: bool) -> Result<Option<StartPoint>> {
    let Some(root) = repo_root(dir) else {
        return Ok(None);
    };
    let dirty = content_ids(&root, &dirty_paths(&root)?)?;
    let (untracked, untracked_partial) = if list {
        let (paths, complete) = untracked_paths(&root, UNTRACKED_TIME, UNTRACKED_MAX);
        (Some(paths), !complete)
    } else {
        (None, false)
    };
    Ok(Some(StartPoint {
        recorded_at: now,
        head: head(&root),
        repo_root: root,
        dirty,
        untracked,
        untracked_partial,
    }))
}

/// The paths that git does not track, ignored paths too. `--directory` gives one
/// line for a folder with no tracked file, so a large ignored folder such as
/// `node_modules` costs one line. The list stops at `limit` or at `max` paths.
/// The second value is false when the list stopped or git failed.
fn untracked_paths(root: &Path, limit: Duration, max: usize) -> (Vec<String>, bool) {
    use std::io::Read;
    let deadline = Instant::now() + limit;
    let child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--others", "--directory", "-z"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return (Vec::new(), false);
    };
    let Some(mut out) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return (Vec::new(), false);
    };
    // Read on a separate thread, so that the time limit works while git is slow.
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 65536];
        let mut part = Vec::new();
        while let Ok(n) = out.read(&mut buf) {
            if n == 0 {
                break;
            }
            for &b in &buf[..n] {
                if b == 0 {
                    let path = String::from_utf8_lossy(&part).into_owned();
                    if tx.send(Some(path)).is_err() {
                        return;
                    }
                    part.clear();
                } else {
                    part.push(b);
                }
            }
        }
        let _ = tx.send(None);
    });
    let mut paths = Vec::new();
    let complete = loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Some(p)) => {
                paths.push(p);
                if paths.len() >= max {
                    break false;
                }
            }
            Ok(None) => break true,
            Err(_) => break false,
        }
    };
    if !complete {
        let _ = child.kill();
    }
    let ok = child.wait().is_ok_and(|s| s.success());
    (paths, complete && ok)
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

/// The added and removed lines of one file.
#[derive(Debug, Default)]
pub struct FileDiff {
    pub path: String,
    pub status: char,
    /// (line number in the new file, text)
    pub added: Vec<(u32, String)>,
    /// (line number in the old file, text)
    pub removed: Vec<(u32, String)>,
}

/// Line changes of the files that changed during the session.
pub fn line_diffs(start: &StartPoint, changes: &Changes) -> Result<Vec<FileDiff>> {
    let root = &start.repo_root;
    let raw = git(
        root,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--no-renames",
            "--no-color",
            "--no-ext-diff",
            "-U0",
            &changes.base,
        ],
    )?;
    let mut diffs = parse_unified(&String::from_utf8_lossy(&raw));

    // Untracked files are not in `git diff`: all their lines are added.
    for f in changes.files.iter().filter(|f| f.status == 'A') {
        if diffs.iter().any(|d| d.path == f.path) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(&f.path)) else {
            continue;
        };
        let added = text
            .lines()
            .enumerate()
            .map(|(i, l)| (i as u32 + 1, l.to_string()))
            .collect();
        diffs.push(FileDiff {
            path: f.path.clone(),
            status: 'A',
            added,
            removed: Vec::new(),
        });
    }

    let session: Vec<&str> = changes.session_files().map(|f| f.path.as_str()).collect();
    diffs.retain(|d| session.contains(&d.path.as_str()));
    for d in &mut diffs {
        if let Some(f) = changes.files.iter().find(|f| f.path == d.path) {
            d.status = f.status;
        }
    }
    Ok(diffs)
}

/// Parse `git diff -U0` output.
fn parse_unified(text: &str) -> Vec<FileDiff> {
    let mut out: Vec<FileDiff> = Vec::new();
    let (mut old_line, mut new_line) = (0u32, 0u32);
    let mut old_path = String::new();
    // `---` and `+++` are headers only before the first hunk of a file.
    let mut in_header = false;
    for raw in text.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with("diff --git ") {
            in_header = true;
            out.push(FileDiff {
                status: 'M',
                ..FileDiff::default()
            });
        } else if let (true, Some(p)) = (in_header, line.strip_prefix("--- ")) {
            old_path = p.strip_prefix("a/").unwrap_or(p).to_string();
        } else if let (true, Some(p)) = (in_header, line.strip_prefix("+++ ")) {
            if let Some(d) = out.last_mut() {
                d.path = if p == "/dev/null" {
                    old_path.clone()
                } else {
                    p.strip_prefix("b/").unwrap_or(p).to_string()
                };
            }
        } else if let Some(h) = line.strip_prefix("@@ ") {
            in_header = false;
            // @@ -old[,n] +new[,n] @@
            let mut parts = h.split_whitespace();
            let start = |s: Option<&str>| {
                s.and_then(|s| s[1..].split(',').next()?.parse::<u32>().ok())
                    .unwrap_or(0)
            };
            old_line = start(parts.next());
            new_line = start(parts.next());
        } else if let Some(d) = out.last_mut() {
            if let Some(t) = line.strip_prefix('+') {
                d.added.push((new_line, t.to_string()));
                new_line += 1;
            } else if let Some(t) = line.strip_prefix('-') {
                d.removed.push((old_line, t.to_string()));
                old_line += 1;
            }
        }
    }
    out.retain(|d| !d.path.is_empty());
    out
}

/// A stable hash of some bytes, as hex.
pub fn hash_hex(bytes: &[u8]) -> String {
    let mut h = Fnv::new();
    h.write(bytes);
    format!("{:016x}", h.0)
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

/// All paths at the start (lowercase): tracked files, dirty files and untracked paths. A
/// folder that git did not track is one path without the end `/`. None when the
/// engine cannot know all paths: git failed, or the list of untracked paths is
/// missing or partial.
pub fn paths_at_start(start: &StartPoint) -> Option<Vec<String>> {
    if start.untracked_partial {
        return None;
    }
    let mut paths: Vec<String> = start.dirty.keys().map(|p| p.to_lowercase()).collect();
    paths.extend(
        start
            .untracked
            .as_ref()?
            .iter()
            .map(|p| p.trim_end_matches('/').to_lowercase()),
    );
    if let Some(head) = &start.head {
        let raw = git(
            &start.repo_root,
            &["ls-tree", "-r", "-z", "--name-only", head],
        )
        .ok()?;
        paths.extend(split_nul(&raw).into_iter().map(|p| p.to_lowercase()));
    }
    Some(paths)
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
    fn lists_untracked_folders_as_one_line_and_stops_at_the_limits() {
        let tmp = repo();
        let d = tmp.path();
        write(d, ".gitignore", "cache/\n");
        std::fs::create_dir_all(d.join("cache/deep")).unwrap();
        for i in 0..50 {
            write(d, &format!("cache/deep/{i}.bin"), "x");
        }
        write(d, "notes.txt", "x");
        let (mut paths, complete) = untracked_paths(d, Duration::from_secs(10), 100);
        paths.sort();
        assert!(complete);
        assert_eq!(paths, [".gitignore", "cache/", "notes.txt"]);
        let (paths, complete) = untracked_paths(d, Duration::from_secs(10), 2);
        assert_eq!((paths.len(), complete), (2, false));
        let (_, complete) = untracked_paths(d, Duration::ZERO, 100);
        assert!(!complete, "no time left");
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
    fn parses_unified_diff_with_tricky_lines() {
        let text = "diff --git a/q.sql b/q.sql\n--- a/q.sql\n+++ b/q.sql\n@@ -3,2 +3 @@\n--- old comment\n-x\n+y\r\ndiff --git a/gone.rs b/gone.rs\n--- a/gone.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-fn a() {}\n";
        let d = parse_unified(text);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].path, "q.sql");
        assert_eq!(
            d[0].removed,
            [(3, "-- old comment".to_string()), (4, "x".to_string())]
        );
        assert_eq!(d[0].added, [(3, "y".to_string())]);
        assert_eq!(d[1].path, "gone.rs");
        assert_eq!(d[1].removed, [(1, "fn a() {}".to_string())]);
    }

    #[test]
    fn line_diffs_cover_commits_and_new_files_of_the_session_only() {
        let tmp = repo();
        let d = tmp.path();
        write(d, "a.txt", "one\n");
        write(d, "user.txt", "one\n");
        run(d, &["add", "."]);
        run(d, &["commit", "-q", "-m", "first"]);
        write(d, "user.txt", "one\nuser\n");
        let start = record(d, 0).unwrap().unwrap();

        write(d, "a.txt", "one\ntwo\n");
        run(d, &["commit", "-q", "-m", "agent", "--", "a.txt"]);
        write(d, "new.txt", "x\n");

        let c = since(&start).unwrap();
        let diffs = line_diffs(&start, &c).unwrap();
        let paths: Vec<_> = diffs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "new.txt"]);
        assert_eq!(diffs[0].added, [(2, "two".to_string())]);
        assert_eq!(diffs[1].status, 'A');
    }

    #[test]
    fn a_folder_outside_git_has_no_start_point() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(record(tmp.path(), 0).unwrap().is_none());
    }
}
