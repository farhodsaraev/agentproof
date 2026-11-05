//! Test commands and their results.
//!
//! A test command is a shell command that runs a test runner. The result of a
//! test run comes first from the summary that the runner writes in its output.
//! The exit code counts only when the shell cannot hide it, for example with a
//! pipe. The text of a tool result is what the agent saw: a result that is not
//! in the transcript was also not visible to the agent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

/// The result of one test run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Passed,
    Failed,
    /// The runner found no tests to run.
    NoTests,
    /// The transcript does not show the result.
    Unknown,
}

/// The evidence for an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// The summary of the test runner in the output.
    Summary,
    /// The exit code of the command.
    ExitCode,
    /// The shell can hide the exit code, and the output has no summary.
    HiddenExitCode,
    /// The command ran in the background, and the transcript has no summary.
    Background,
    /// The transcript has no result for the command.
    NoResult,
}

/// A shell command that runs tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCommand {
    /// The runner, for example `jest`, `cargo test` or `script test:unit`.
    pub runner: String,
    /// An exit code of 0 does not prove a pass: a pipe without `pipefail`, a
    /// later command after `;`, `||`, `&` or a new line, or `||` before the runner.
    pub pass_hidden: bool,
    /// A failure exit code can come from a different command: a pipe without
    /// `pipefail`, or any later command.
    pub fail_hidden: bool,
    /// The paths, names and filter options that select the tests, for example
    /// `tests/test_a.py` or `-k login`. Empty for a run of all tests.
    pub filters: Vec<Filter>,
    /// The folder that the command moves to before the runner starts, if the
    /// command shows it. See `resolve`.
    pub dir: Option<String>,
}

/// What the output of a test run says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// `Passed`, `Failed` or `NoTests`.
    pub outcome: Outcome,
    /// The line of the output that shows the outcome.
    pub line: String,
    /// The runner that writes this kind of summary line with counts, for
    /// example `pytest` or `cargo test`. None for other lines.
    pub family: Option<&'static str>,
    /// The output shows that a filter left out tests, for example
    /// `3 deselected` from pytest or `3 filtered out` from cargo.
    pub narrowed: bool,
}

/// A test command and its result. The JSON file keeps this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestRun {
    pub runner: String,
    /// An exit code of 0 does not prove a pass, for example because of a pipe.
    pub exit_code_hidden: bool,
    pub outcome: Outcome,
    pub basis: Basis,
    /// The line of the output that shows the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The filters of the command. Empty for a run of all tests.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    /// The output shows that a filter left out tests.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub narrowed: bool,
    /// The folder that the command moved to. Not in the JSON file.
    #[serde(skip)]
    pub dir: Option<String>,
}

/// The result of a command in the transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The transcript has no result.
    Missing,
    /// The command ran in the background. The code comes from the notice at its end.
    Background(Option<i32>),
    /// The command ended. `failed` is the error mark of the tool result.
    Ended { failed: bool },
}

/// The outcome of a test command from its summary and its exit.
pub fn decide(cmd: &TestCommand, exit: Exit, summary: Option<&Summary>) -> TestRun {
    let (outcome, basis) = match (summary, exit) {
        (Some(s), _) if s.outcome != Outcome::Passed => (s.outcome, Basis::Summary),
        // The tests passed, but the runner failed, for example at a coverage limit.
        (Some(_), Exit::Ended { failed: true }) if !cmd.fail_hidden => {
            (Outcome::Failed, Basis::ExitCode)
        }
        (Some(_), Exit::Background(Some(code))) if code != 0 && !cmd.fail_hidden => {
            (Outcome::Failed, Basis::ExitCode)
        }
        (Some(_), _) => (Outcome::Passed, Basis::Summary),
        (None, Exit::Missing) => (Outcome::Unknown, Basis::NoResult),
        (None, Exit::Background(Some(code))) if code != 0 && !cmd.fail_hidden => {
            (Outcome::Failed, Basis::ExitCode)
        }
        // A background command can report exit code 0 after a failure.
        (None, Exit::Background(_)) => (Outcome::Unknown, Basis::Background),
        (None, Exit::Ended { failed: true }) if !cmd.fail_hidden => {
            (Outcome::Failed, Basis::ExitCode)
        }
        (None, Exit::Ended { failed: false }) if !cmd.pass_hidden => {
            (Outcome::Passed, Basis::ExitCode)
        }
        (None, Exit::Ended { .. }) => (Outcome::Unknown, Basis::HiddenExitCode),
    };
    TestRun {
        runner: cmd.runner.clone(),
        exit_code_hidden: cmd.pass_hidden,
        outcome,
        basis,
        summary: summary.map(|s| crate::redact::redact(&s.line).0),
        filters: cmd.filters.clone(),
        narrowed: summary.is_some_and(|s| s.narrowed),
        dir: cmd.dir.clone(),
    }
}

/// True if a later run ran the same tests as an earlier run, or more tests.
pub fn covers(later: &TestRun, earlier: &TestRun) -> bool {
    // The output of the later run shows left-out tests, but its command shows no
    // reason, for example because of a setting of the project. Then the output of
    // the earlier run must show the same, with no option that leaves out other tests.
    let has = |r: &TestRun, roles: &[Role]| r.filters.iter().any(|f| roles.contains(&f.role));
    let explained = has(later, &[Role::Only, Role::Name]);
    let same_setting = earlier.narrowed && !has(earlier, &[Role::Only]);
    selects_all(later, earlier) && (!later.narrowed || explained || same_setting)
}

/// True if the filters of a later run select all tests that the filters of an
/// earlier run select. The two runs have the same runner.
pub fn selects_all(later: &TestRun, earlier: &TestRun) -> bool {
    fn texts(r: &TestRun, role: Role) -> BTreeSet<&str> {
        r.filters
            .iter()
            .filter(|f| f.role == role)
            .map(|f| f.text.as_str())
            .collect()
    }
    // The same project, settings and mode.
    if texts(later, Role::Place) != texts(earlier, Role::Place) {
        return false;
    }
    // Each option of the later run that leaves out tests applies to the earlier run too.
    let earlier_only = texts(earlier, Role::Only);
    if !texts(later, Role::Only)
        .iter()
        .all(|l| earlier_only.iter().any(|e| looser(l, e)))
    {
        return false;
    }
    let (mut l, mut e) = (groups(later), groups(earlier));
    // Runs in two folders: one folder must hold the other. The paths of the run in
    // the inner folder then start with the way to it from the outer folder. The
    // engine compares two absolute folders, or two folders relative to the start
    // folder of the shell. It does not know the start folder.
    if let (Some(ld), Some(ed)) = (&later.dir, &earlier.dir) {
        if ld != ed && is_absolute(ld) == is_absolute(ed) {
            let (inner, rel) = match (inside(ed, ld), inside(ld, ed)) {
                (Some(rel), _) => (&mut e, rel),
                (_, Some(rel)) => (&mut l, rel),
                _ => return false,
            };
            let paths = inner.entry("").or_default();
            if paths.is_empty() {
                // A go run with no package filter runs all packages in the folder.
                paths.push(if later.runner == "go test" {
                    format!("{rel}/...")
                } else {
                    rel.to_string()
                });
            } else {
                for p in paths.iter_mut() {
                    *p = if p == "." {
                        rel.to_string()
                    } else {
                        format!("{rel}/{p}")
                    };
                }
            }
        }
    }
    let exact = later.filters.iter().any(|f| f.text == "--exact");
    // Each group of the later run (paths, packages, targets, names) holds the earlier tests.
    l.iter().all(|(group, ls)| {
        e.get(group).is_some_and(|es| {
            es.iter().all(|e| {
                ls.iter()
                    .any(|l| includes(&later.runner, group, l, e, exact))
            })
        })
    })
}

// ---------- Which commands run tests ----------

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid pattern")
}

/// The runner of a test command, if the command runs tests.
pub fn detect(command: &str) -> Option<TestCommand> {
    let prepared = prepare(command);
    let text = finish(&blank_quotes(&prepared));
    let pipefail = pipefail_on(&text);
    let lists = split_lists(&text);
    // The same parts with the quoted text kept, for the filters and the folder of the run.
    let kept = split_lists(&finish(&keep_quotes(&prepared)));
    for (i, (pipeline, before)) in lists.iter().enumerate() {
        let stages: Vec<&str> = pipeline.split('|').collect();
        for (j, stage) in stages.iter().enumerate() {
            let Some(runner) = runner_of(stage) else {
                continue;
            };
            let kept_stage = kept
                .get(i)
                .and_then(|(p, _)| p.split('|').nth(j))
                .unwrap_or(stage);
            let words = clean_words(kept_stage);
            let filters = filters_of(&runner, &words);
            let mut moves = moves_before(&lists, &kept, i);
            moves.extend(folder_options(&runner, &words));
            let piped = j + 1 < stages.len();
            let after = &lists[i + 1..];
            let later = after.iter().any(|(p, _)| !p.trim().is_empty());
            let later_unless_and = after
                .iter()
                .any(|(p, sep)| !p.trim().is_empty() && *sep != Some("&&"));
            // `cmd &` runs the command in the shell background.
            let shell_background = after.first().is_some_and(|(_, sep)| *sep == Some("&"));
            let or_before = *before == Some("||");
            // `! cmd` turns a failure into exit code 0.
            let negated = pipeline.trim_start().starts_with('!');
            return Some(TestCommand {
                runner,
                pass_hidden: (piped && !pipefail)
                    || later_unless_and
                    || shell_background
                    || or_before
                    || negated,
                // With `pipefail`, a later stage such as `grep` can still give the failure.
                fail_hidden: piped || later || negated || can_stop_before(&lists, i),
                filters,
                dir: resolve(&moves),
            });
        }
    }
    None
}

/// `set -o pipefail` or `set -euo pipefail`. `set +o pipefail` turns it off.
fn pipefail_on(text: &str) -> bool {
    static ON: OnceLock<Regex> = OnceLock::new();
    ON.get_or_init(|| re(r"\bset\s+-[A-Za-z]*o\s+pipefail\b"))
        .is_match(text)
        && !text.contains("+o pipefail")
}

/// True if a command before `&&` can fail. Then the runner does not run, and the
/// exit code comes from that command. `cd`, `export` and assignments almost never fail.
fn can_stop_before(lists: &[(String, Option<&'static str>)], i: usize) -> bool {
    let mut k = i;
    while k > 0 && lists[k].1 == Some("&&") {
        k -= 1;
        let p = lists[k]
            .0
            .trim()
            .trim_start_matches(['(', '{'])
            .trim_start();
        if !p.is_empty() && !is_setup(p) {
            return true;
        }
    }
    false
}

/// The command without here-documents, and with the text of `bash -c "..."`
/// as part of the command.
fn prepare(command: &str) -> String {
    static INLINE: OnceLock<Regex> = OnceLock::new();
    let c = crate::checks::strip_heredocs(command);
    INLINE
        .get_or_init(|| {
            re(r#"\b(?:bash|sh|zsh)(?:\s+-[a-z]+)*\s+-[a-z]*c\s+(?:"((?:[^"\\]|\\.)*)"|'([^']*)')"#)
        })
        .replace_all(&c, " ( ${1}${2} ) ")
        .into_owned()
}

/// The text without the `&` and `|` of redirections. `2>&1` goes away, and
/// `&> file` becomes `> file`, so that the file is not a test filter.
fn finish(text: &str) -> String {
    static DUP: OnceLock<Regex> = OnceLock::new();
    DUP.get_or_init(|| re(r"\d?>&\d"))
        .replace_all(text, " ")
        .replace("&>", ">")
        .replace(">|", ">")
        .replace("|&", "|")
}

/// Quoted text becomes `''` or `""`, so that `|` or `;` in a string is not a
/// separator. An apostrophe inside double quotes stays inside them. A comment
/// ends at the end of its line.
fn blank_quotes(text: &str) -> String {
    scan_quotes(text, false)
}

/// Quoted text stays, but the separators in it become spaces. The parts of the
/// command are then the same as with `blank_quotes`.
fn keep_quotes(text: &str) -> String {
    scan_quotes(text, true)
}

fn scan_quotes(text: &str, keep: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut prev = ' ';
    let quoted = |out: &mut String, quote: char, body: &str| {
        out.push(quote);
        if keep {
            out.extend(
                body.chars()
                    .map(|c| if "|;&\n".contains(c) { ' ' } else { c }),
            );
        }
        out.push(quote);
    };
    while let Some(c) = chars.next() {
        match c {
            // An escaped separator, for example `\;` in `find -exec`, is not a separator.
            '\\' if chars.peek().is_some_and(|n| "|;&\n'\" ".contains(*n)) => {
                out.push('_');
                chars.next();
            }
            '\'' => {
                let mut body = String::new();
                for n in chars.by_ref() {
                    if n == '\'' {
                        break;
                    }
                    body.push(n);
                }
                quoted(&mut out, '\'', &body);
            }
            '"' => {
                let mut body = String::new();
                let mut escaped = false;
                for n in chars.by_ref() {
                    match n {
                        _ if escaped => escaped = false,
                        '\\' => escaped = true,
                        '"' => break,
                        _ => {}
                    }
                    body.push(n);
                }
                quoted(&mut out, '"', &body);
            }
            '#' if prev.is_whitespace() => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
        prev = c;
    }
    out
}

/// The pipelines of a command list, each with the separator before it.
fn split_lists(text: &str) -> Vec<(String, Option<&'static str>)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut before = None;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let sep = match (c, next) {
            ('&', Some('&')) => Some(("&&", 2)),
            ('|', Some('|')) => Some(("||", 2)),
            (';', _) => Some((";", 1)),
            ('\n', _) => Some(("\n", 1)),
            ('&', _) => Some(("&", 1)),
            _ => None,
        };
        match sep {
            Some((s, len)) => {
                out.push((std::mem::take(&mut current), before));
                before = Some(s);
                i += len;
            }
            None => {
                current.push(c);
                i += 1;
            }
        }
    }
    out.push((current, before));
    out
}

/// The test runner that one pipeline stage starts, if any.
fn runner_of(stage: &str) -> Option<String> {
    static ENV: OnceLock<Regex> = OnceLock::new();
    static WRAPPER: OnceLock<Regex> = OnceLock::new();
    static PACKAGE_RUNNER: OnceLock<Regex> = OnceLock::new();
    static CONTAINER: OnceLock<Regex> = OnceLock::new();
    let env = ENV.get_or_init(|| re(r"^(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s+)+"));
    let wrapper = WRAPPER.get_or_init(|| {
        re(r"^(?:time|exec|env|nice|command|stdbuf\s+\S+|timeout(?:\s+-\S+)*\s+\S+|cross-env|xvfb-run(?:\s+-\S+)*|dotenv(?:\s+-\S+)*\s+--)\s+")
    });
    let package_runner = PACKAGE_RUNNER.get_or_init(|| {
        re(r"^(?:(?:npx|bunx|pnpx|uvx)(?:\.cmd|\.exe)?|pipx\s+run|(?:npm|pnpm|yarn)(?:\.cmd)?(?:\s+(?:--dir|-C|--prefix|--filter|-F|-w|--workspace)\s+\S+)*\s+(?:exec|dlx)|uv\s+run|poetry\s+run|pipenv\s+run|hatch\s+run|pdm\s+run|rye\s+run)(?:\s+-\S+)*\s+")
    });
    let mut s = stage
        .trim_start_matches(|c: char| c.is_whitespace() || "({!".contains(c))
        .trim_end()
        .to_string();
    for _ in 0..6 {
        let before = s.clone();
        s = env.replace(&s, "").into_owned();
        s = wrapper.replace(&s, "").into_owned();
        s = package_runner.replace(&s, "").into_owned();
        if s == before {
            break;
        }
    }
    if let Some(r) = known_runner(&s).or_else(|| package_script(&s)) {
        return (!runs_no_tests(&r, &s)).then_some(r);
    }
    // A runner inside a container command, for example `docker compose run web pytest`.
    let container = CONTAINER.get_or_init(|| re(r"^(?:docker(?:-compose)?|podman)(?:\s|$)"));
    if container.is_match(&s) {
        for (i, _) in s.match_indices(' ') {
            if let Some(r) = runner_of(&s[i + 1..]) {
                return Some(r);
            }
        }
    }
    None
}

/// The end of a runner name: a space, `)`, `;` or the end of the text.
const END: &str = r"(?:[\s);]|$)";

fn known_runner(s: &str) -> Option<String> {
    static RUNNERS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let runners = RUNNERS.get_or_init(|| {
        // A path before the program, for example `.venv/bin/` or `node_modules\.bin\`.
        const P: &str = r"(?:\S*[/\\])?";
        // A Python program, for example `python3.12` or `py.exe`.
        const PY: &str = r"(?:python(?:\d+(?:\.\d+)*)?|py)(?:\.exe)?";
        [
            (format!(r"^{P}(?:pytest|py\.test)(?:\.exe)?"), "pytest"),
            (format!(r"^{P}{PY}(?:\s+-\S+)*\s+-m\s+pytest"), "pytest"),
            (format!(r"^{P}{PY}(?:\s+-\S+)*\s+-m\s+unittest"), "unittest"),
            (format!(r"^{P}{PY}(?:\s+-\S+)*\s+-m\s+nose2?"), "nose"),
            (format!(r"^{P}{PY}\s+\S*manage\.py\s+test"), "django test"),
            (
                format!(r"^{P}cargo(?:\.exe)?(?:\s+\+\S+)?(?:\s+-\S+)*\s+(?:test|t)"),
                "cargo test",
            ),
            (
                format!(r"^{P}cargo(?:\.exe)?(?:\s+\+\S+)?(?:\s+-\S+)*\s+nextest"),
                "cargo nextest",
            ),
            (format!(r"^{P}go(?:\.exe)?\s+test"), "go test"),
            (format!(r"^{P}dotnet(?:\.exe)?\s+test"), "dotnet test"),
            (
                format!(r"^{P}mvnw?(?:\.cmd)?\s(?:.*\s)?(?:test|verify)"),
                "maven test",
            ),
            (
                // `:api:test` is the test task of one project.
                format!(r"^{P}gradlew?(?:\.bat)?\s(?:.*[\s:])?(?:test|check)"),
                "gradle test",
            ),
            (
                r"^make\s(?:.*\s)?(?:tests?|test[-_:]\S+|check)".to_string(),
                "make test",
            ),
            (format!(r"^{P}jest(?:@\S+)?"), "jest"),
            (format!(r"^{P}vitest(?:@\S+)?"), "vitest"),
            (format!(r"^{P}mocha(?:@\S+)?"), "mocha"),
            (format!(r"^{P}ava(?:@\S+)?"), "ava"),
            (
                format!(r"^{P}playwright(?:@\S+)?\s+test"),
                "playwright test",
            ),
            (r"^(?:bundle\s+exec\s+)?rspec".to_string(), "rspec"),
            (format!(r"^{P}phpunit"), "phpunit"),
            (r"^(?:tox|nox)".to_string(), "tox"),
            (r"^ctest".to_string(), "ctest"),
            (r"^bats".to_string(), "bats"),
            (r"^deno\s+test".to_string(), "deno test"),
            (r"^mix\s+test".to_string(), "mix test"),
            (r"^bun\s+test".to_string(), "bun test"),
            (r"^node\s(?:.*\s)?--test".to_string(), "node --test"),
            (
                r"^nx\s+(?:test|run\s+\S+:test\S*|run-many\s.*test\S*|affected\s.*test\S*)"
                    .to_string(),
                "nx test",
            ),
            (r"^turbo\s+(?:run\s+)?test".to_string(), "turbo test"),
            (r"^just\s+test".to_string(), "just test"),
            (r"^bazel(?:isk)?\s+test".to_string(), "bazel test"),
        ]
        .into_iter()
        .map(|(p, k)| (re(&format!("{p}{END}")), k))
        .collect()
    });
    runners
        .iter()
        .find(|(r, _)| r.is_match(s))
        .map(|(_, k)| (*k).to_string())
}

/// A test script of `package.json`, or a test runner, that npm, pnpm, yarn or
/// bun starts. The words are read in order: the manager, its options, the
/// subcommand and the script name.
fn package_script(s: &str) -> Option<String> {
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    // Options of the manager that take a value.
    const VALUE_OPTIONS: &[&str] = &[
        "--filter",
        "-F",
        "--prefix",
        "-C",
        "--dir",
        "--cwd",
        "-w",
        "--workspace",
    ];
    let words: Vec<&str> = s.split_whitespace().collect();
    let manager = words
        .first()?
        .rsplit(['/', '\\'])
        .next()?
        .trim_end_matches(".cmd")
        .trim_end_matches(".exe");
    if !matches!(manager, "npm" | "pnpm" | "yarn" | "bun") {
        return None;
    }
    let mut i = 1;
    while let Some(w) = words.get(i) {
        if VALUE_OPTIONS.contains(w) || (manager == "yarn" && *w == "workspace") {
            i += 2;
        } else if w.starts_with('-') && *w != "--" {
            i += 1;
        } else {
            break;
        }
    }
    let sub = *words.get(i)?;
    let script = match sub {
        "test" | "t" | "tst" => "test",
        "run" | "run-script" => {
            let mut j = i + 1;
            while words
                .get(j)
                .is_some_and(|w| w.starts_with('-') && *w != "--")
            {
                j += 1;
            }
            *words.get(j)?
        }
        // yarn, pnpm and bun also start a script or a program by its name.
        _ if manager != "npm" => {
            if let Some(r) = known_runner(&words[i..].join(" ")) {
                return Some(r);
            }
            sub
        }
        _ => return None,
    };
    SCRIPT
        .get_or_init(|| re(r"^(?:test(?:[:_-][\w.:-]+)?|[\w.-]+:test[\w.:-]*)$"))
        .is_match(script)
        .then(|| format!("script {script}"))
}

/// True if the command only builds, lists or explains the tests. Then it runs no test.
fn runs_no_tests(runner: &str, stage: &str) -> bool {
    static ANY: OnceLock<Regex> = OnceLock::new();
    static GRADLE: OnceLock<Regex> = OnceLock::new();
    static GO: OnceLock<Regex> = OnceLock::new();
    let any = ANY.get_or_init(|| {
        re(r"(?:^|\s)(?:--version|--help|-h|--list|--list-tests|--listTests|--showConfig|--no-run|--collect-only|--co)(?:[\s=]|$)")
    });
    let gradle = GRADLE.get_or_init(|| re(r"(?:^|\s)(?:-x|--exclude-task)[\s=]+\S*test"));
    let go = GO.get_or_init(|| re(r"(?:^|\s)-(?:c|list)(?:[\s=]|$)"));
    any.is_match(stage)
        || (runner == "gradle test" && gradle.is_match(stage))
        || (runner == "maven test"
            && (stage.contains("-DskipTests") || stage.contains("-Dmaven.test.skip")))
        || (runner == "go test" && go.is_match(stage))
        || (runner == "vitest" && stage.split_whitespace().any(|w| w == "list"))
        || (runner == "make test"
            && stage
                .split_whitespace()
                .any(|w| matches!(w, "-n" | "--dry-run" | "--just-print" | "--recon")))
        || (runner == "ctest"
            && stage
                .split_whitespace()
                .any(|w| matches!(w, "-N" | "--show-only")))
}

// ---------- Which tests a run selects ----------

/// How a filter selects tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A path, a module, a package or a target. The filters of one group are a
    /// union: the run selects the tests of each filter. The group is a name such
    /// as "-p" for cargo packages, or "" for paths.
    Scope(&'static str),
    /// A part of the test names, for example `parser` in `cargo test parser`. A union.
    Name,
    /// An option that leaves out tests, for example `-k`, `-m` or `--skip`.
    Only,
    /// The project, the settings or the mode of the run. Two runs must have the same.
    Place,
}

/// A filter of a test command, for example `tests/test_a.py` or `-k login`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub role: Role,
    pub text: String,
}

impl Filter {
    fn new(role: Role, text: impl Into<String>) -> Self {
        Filter {
            role,
            text: text.into(),
        }
    }
}

/// The JSON file shows a filter as its text.
impl Serialize for Filter {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text)
    }
}

/// The group of the name filters.
const NAMES: &str = "name";

/// The filters of a run that select tests by a union, by group: paths (""),
/// packages, targets and names.
fn groups(r: &TestRun) -> BTreeMap<&'static str, Vec<String>> {
    let mut out: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for f in &r.filters {
        let group = match f.role {
            Role::Scope(g) => g,
            Role::Name => NAMES,
            Role::Only | Role::Place => continue,
        };
        out.entry(group).or_default().push(f.text.clone());
    }
    out
}

/// The way from a folder to a folder in it, for example `backend` from `c:/repo`
/// to `c:/repo/backend`.
fn inside<'a>(dir: &'a str, outer: &str) -> Option<&'a str> {
    dir.strip_prefix(outer)?
        .strip_prefix('/')
        .filter(|rel| !rel.is_empty())
}

/// True if the filter `l` of a group selects all tests that the filter `e` selects.
fn includes(runner: &str, group: &str, l: &str, e: &str, exact: bool) -> bool {
    // `e` continues `l` after one of the characters.
    let below = |after: &[char]| {
        e.strip_prefix(l)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| after.contains(&c))
    };
    if l == e {
        return true;
    }
    match group {
        // cargo and nextest select each test whose name contains the filter.
        NAMES => !exact && e.contains(l),
        // cargo: `--tests` holds the library, the programs and the test files.
        "target" => match l {
            "--tests" => {
                e == "--lib" || e == "--bins" || e.starts_with("--bin ") || e.starts_with("--test ")
            }
            "--bins" => e.starts_with("--bin "),
            "--examples" => e.starts_with("--example "),
            "--benches" => e.starts_with("--bench "),
            _ => false,
        },
        // gradle: `test` holds the test task of each project, for example `:api:test`.
        "task" => {
            e.ends_with(&format!(":{l}")) || (l == "check" && (e == "test" || e.ends_with(":test")))
        }
        "" => match l.strip_suffix("/...") {
            // go `./pkg/...` and bazel `//pkg/...`: the folder and all folders in it.
            Some(base) => {
                e == base
                    || e.strip_prefix(base)
                        .is_some_and(|rest| rest.starts_with(['/', ':']))
            }
            // A shell pattern such as `tests/test_game*.py` holds each file that it matches.
            None if l.contains(['*', '?']) => {
                let file = e.split("::").next().unwrap_or(e);
                let file = match file.rsplit_once(':') {
                    Some((f, line)) if line.chars().all(|c| c.is_ascii_digit()) => f,
                    _ => file,
                };
                glob_match(l, file)
            }
            // A go package does not hold the packages in its folders.
            None if runner == "go test" => false,
            // These runners select each test file whose path contains the filter.
            None if matches!(runner, "jest" | "vitest" | "playwright test" | "bun test") => {
                e.contains(l)
            }
            // A Python module holds the modules in it: `blog` holds `blog.tests`.
            None if matches!(runner, "unittest" | "django test") => below(&['/', '.']),
            // A folder holds its files. A file holds its tests (`a.py::test_x`) and
            // its lines (`a_spec.rb:10`).
            None => below(&['/', ':']),
        },
        _ => false,
    }
}

/// True if a path matches a shell pattern with `*` and `?`. These do not match `/`.
fn glob_match(pattern: &str, path: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), path.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    // The last `*` and the position in the path where it started.
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == t[ti] || (p[pi] == '?' && t[ti] != '/')) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star.filter(|&(_, st)| t[st] != '/') {
            // The `*` takes one more character.
            star = Some((sp, st + 1));
            pi = sp + 1;
            ti = st + 1;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// True if the option `l` of a later run keeps each test that the option `e` of
/// an earlier run keeps. A name pattern keeps more tests than a longer pattern:
/// `-k login` keeps each test of `-k test_login_ok`.
fn looser(l: &str, e: &str) -> bool {
    const PATTERNS: &[&str] = &["-k", "-t", "--grep", "-run", "--filter", "--example"];
    if l == e {
        return true;
    }
    let (Some((lf, lv)), Some((ef, ev))) = (l.split_once(' '), e.split_once(' ')) else {
        return false;
    };
    // A plain word, not an expression or a pattern with special characters.
    let plain = |v: &str| {
        !v.is_empty()
            && !v.contains(|c: char| "^$.*+?()[]{}|\\=!&,".contains(c) || (lf == "-k" && c == ' '))
    };
    let ev = ev.trim_start_matches('^').trim_end_matches('$');
    lf == ef && PATTERNS.contains(&lf) && plain(lv) && plain(ev) && ev.contains(lv)
}

/// What an option of a test runner does.
#[derive(Clone, Copy)]
enum Opt {
    /// It takes a value that selects no tests, for example `--tb short`.
    Value,
    /// It takes a value only if the next word is a number or `auto`, for example `--bail 1`.
    Count,
    /// It takes a value that is a filter with this role. The text of the filter is
    /// the name given here and the value, for example `-t adds`.
    Filter(Role, &'static str),
    /// Like `Filter`, for a list with commas, for example `-pl api,web`.
    List(Role, &'static str),
    /// It is a filter with this role and no value, for example `--lf`.
    Flag(Role, &'static str),
    /// It takes a test file or a test folder, for example `--spec a.test.js`.
    Path,
}

/// A table of the options of a runner.
type Options = &'static [(&'static str, Opt)];

/// An option and its value in the same word, for example `--tb=short`.
fn split_option(word: &str) -> (&str, Option<&str>) {
    match word.split_once('=') {
        Some((flag, value)) if flag.starts_with('-') => (flag, Some(value)),
        _ => (word, None),
    }
}

/// The words before `--` and the words after it.
fn split_at_dashes(args: &[String]) -> (&[String], &[String]) {
    match args.iter().position(|a| a == "--") {
        Some(p) => (&args[..p], &args[p + 1..]),
        None => (args, &[]),
    }
}

/// The filters in the words after a runner, from tables of its options. A word
/// that is not an option or the value of an option goes to `positional`.
fn read(
    args: &[String],
    tables: &[Options],
    mut positional: impl FnMut(&str, &mut Vec<Filter>),
) -> Vec<Filter> {
    let text = |name: &str, value: &str| {
        if name.is_empty() {
            value.to_string()
        } else {
            format!("{name} {value}")
        }
    };
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let (flag, inline) = split_option(a);
        let opt = tables
            .iter()
            .flat_map(|t| t.iter())
            .find(|(f, _)| *f == flag)
            .map(|(_, o)| *o);
        let count_next = args
            .get(i + 1)
            .is_some_and(|n| n == "auto" || n.parse::<u32>().is_ok());
        let takes_next = inline.is_none()
            && match opt {
                Some(Opt::Count) => count_next,
                Some(Opt::Flag(..)) | None => false,
                Some(_) => true,
            };
        let value = if takes_next {
            i += 1;
            args.get(i).map_or("", String::as_str)
        } else {
            inline.unwrap_or("")
        };
        match opt {
            Some(Opt::Filter(role, name)) => out.push(Filter::new(role, text(name, value))),
            Some(Opt::List(role, name)) => out.extend(
                value
                    .split(',')
                    .filter(|v| !v.is_empty())
                    .map(|v| Filter::new(role, text(name, v))),
            ),
            Some(Opt::Flag(role, name)) => out.push(Filter::new(role, name)),
            Some(Opt::Path) => push_path(value, &mut out),
            Some(Opt::Value | Opt::Count) => {}
            None if a.starts_with('-') => {}
            None => positional(a, &mut out),
        }
        i += 1;
    }
    out
}

/// The words of a command part. A quoted text is one word without its quotes.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            None => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// The words of a command part without redirections and without the brackets of a
/// group, for example the `)` from `sh -c "..."`. The file of a redirection such as
/// `> out.log`, `2>/dev/null` or `< in.txt` is not a test filter.
fn clean_words(stage: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for w in words(stage) {
        if std::mem::take(&mut skip_next) {
            continue;
        }
        let rest = w.trim_start_matches(|c: char| c.is_ascii_digit() || c == '&');
        if rest.starts_with(['>', '<']) {
            // A redirection with no file in the same word takes the next word.
            skip_next = rest.trim_start_matches(['>', '<']).is_empty();
            continue;
        }
        let w = if w.contains('(') {
            w
        } else {
            w.trim_end_matches(')').to_string()
        };
        if !w.is_empty() && !matches!(w.as_str(), "(" | "{" | "}") {
            out.push(w);
        }
    }
    out
}

/// A program name without its folder, `.exe`, `.cmd` and `@version`.
fn base_name(word: &str) -> String {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    let name = name.split('@').next().unwrap_or(name);
    name.trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .to_ascii_lowercase()
}

/// A path filter in one form: `/` separators, no `./` at the start and no `/` at the end.
fn norm_path(word: &str) -> String {
    let p = word.replace('\\', "/");
    p.trim_start_matches("./").trim_end_matches('/').to_string()
}

/// A path that names the usual test folder or the project folder selects all tests.
fn is_root(path: &str) -> bool {
    matches!(
        path,
        "" | "." | "tests" | "test" | "spec" | "specs" | "__tests__" | "..." | "//..."
    )
}

/// Add a path filter, unless the path names all tests, for example `tests` or `.`.
fn push_path(word: &str, out: &mut Vec<Filter>) {
    let p = norm_path(word);
    if !is_root(&p) {
        out.push(Filter::new(Role::Scope(""), p));
    }
}

fn looks_like_path(word: &str) -> bool {
    word.contains(['/', '\\'])
        || word.contains("::")
        || [
            ".py", ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs", ".rs", ".go", ".rb", ".php",
        ]
        .iter()
        .any(|ext| word.ends_with(ext))
}

/// True if a word names a test file or a test folder, for example `tests/unit`,
/// `test_dates.py`, `a.test.ts` or `user_spec.rb:10`.
fn is_test_path(word: &str) -> bool {
    static FILE: OnceLock<Regex> = OnceLock::new();
    let p = norm_path(word);
    let file = p.split("::").next().unwrap_or(&p);
    let file = match file.rsplit_once(':') {
        Some((f, line)) if !line.is_empty() && line.chars().all(|c| c.is_ascii_digit()) => f,
        _ => file,
    };
    let name = file.rsplit('/').next().unwrap_or(file);
    let in_test_folder = file
        .split('/')
        .any(|c| matches!(c, "tests" | "test" | "spec" | "specs" | "__tests__" | "e2e"));
    match name.rsplit_once('.') {
        Some((_, ext)) => {
            FILE.get_or_init(|| {
                re(r"^(?:test_.+\.py|.+_test\.(?:py|go|rs|exs?)|.+\.(?:test|spec)\.[cm]?[jt]sx?|.+_spec\.rb|.+Tests?\.(?:java|kt|cs|php))$")
            })
            .is_match(name)
                || (in_test_folder
                    && matches!(
                        ext,
                        "py" | "js"
                            | "ts"
                            | "jsx"
                            | "tsx"
                            | "mjs"
                            | "cjs"
                            | "rs"
                            | "go"
                            | "rb"
                            | "php"
                            | "java"
                            | "kt"
                            | "cs"
                            | "ex"
                            | "exs"
                    ))
        }
        None => in_test_folder,
    }
}

/// The test files and test folders that a script gets, for example
/// `tests/test_dates.py` in `./scripts/test.sh tests/test_dates.py`. None if the
/// command part does not start a script. Other programs get files that do not
/// select tests, for example `sed -i ... tests/test_a.py`.
fn script_paths(words: &[String]) -> Option<Vec<Filter>> {
    const INTERPRETERS: &[&str] = &[
        "bash",
        "sh",
        "zsh",
        "py",
        "node",
        "ruby",
        "perl",
        "php",
        "pwsh",
        "powershell",
        "bun",
        "deno",
    ];
    const SCRIPTS: &[&str] = &[
        ".sh", ".bash", ".ps1", ".bat", ".cmd", ".py", ".js", ".mjs", ".rb", ".pl",
    ];
    let program = words.first()?;
    let name = base_name(program);
    let start = if INTERPRETERS.contains(&name.as_str()) || name.starts_with("python") {
        // The script is the first word that is not an option.
        words[1..].iter().position(|w| !w.starts_with('-'))? + 2
    } else if SCRIPTS.iter().any(|ext| program.ends_with(ext))
        || program.starts_with("./")
        || program.contains("scripts/")
        || program.contains("bin/")
    {
        1
    } else {
        return None;
    };
    let mut out = Vec::new();
    for w in &words[start..] {
        if !w.starts_with('-') && is_test_path(w) {
            push_path(w, &mut out);
        }
    }
    Some(out)
}

/// The index of the first word after the runner, for example after `pytest`,
/// `cargo test` or `manage.py test`. The runner words come in this order.
fn args_start(runner: &str, words: &[String]) -> Option<usize> {
    let names: Vec<&[&str]> = match runner {
        "pytest" => vec![&["pytest", "py.test"]],
        "unittest" => vec![&["unittest"]],
        "nose" => vec![&["nose", "nose2"]],
        "django test" => vec![&["manage.py"], &["test"]],
        "cargo test" => vec![&["cargo"], &["test", "t"]],
        "cargo nextest" => vec![&["cargo"], &["nextest"]],
        "go test" => vec![&["go"], &["test"]],
        "dotnet test" => vec![&["dotnet"], &["test"]],
        "deno test" => vec![&["deno"], &["test"]],
        "mix test" => vec![&["mix"], &["test"]],
        "bun test" => vec![&["bun"], &["test"]],
        "bazel test" => vec![&["bazel", "bazelisk"], &["test"]],
        "just test" => vec![&["just"], &["test"]],
        "playwright test" => vec![&["playwright"], &["test"]],
        "node --test" => vec![&["--test"]],
        "maven test" => vec![&["mvn", "mvnw"]],
        "gradle test" => vec![&["gradle", "gradlew"]],
        "make test" => vec![&["make"]],
        "nx test" => vec![&["nx"]],
        "turbo test" => vec![&["turbo"]],
        "jest" => vec![&["jest"]],
        "vitest" => vec![&["vitest"]],
        "mocha" => vec![&["mocha"]],
        "ava" => vec![&["ava"]],
        "rspec" => vec![&["rspec"]],
        "phpunit" => vec![&["phpunit"]],
        "ctest" => vec![&["ctest"]],
        "bats" => vec![&["bats"]],
        "tox" => vec![&["tox", "nox"]],
        _ => return None,
    };
    let mut i = 0;
    for list in names {
        i += words[i..]
            .iter()
            .position(|w| list.contains(&base_name(w).as_str()))?
            + 1;
    }
    Some(i)
}

/// The filters of a test command: the paths, names and options that select
/// tests. An empty list means a run of all tests.
fn filters_of(runner: &str, words: &[String]) -> Vec<Filter> {
    if let Some(script) = runner.strip_prefix("script ") {
        return script_filters(script, words);
    }
    let Some(start) = args_start(runner, words) else {
        return Vec::new();
    };
    let args = &words[start..];
    match runner {
        "pytest" | "nose" => pytest_filters(args),
        "unittest" => unittest_filters(args),
        "django test" => django_filters(args),
        "jest" | "vitest" | "mocha" | "ava" | "playwright test" | "bun test" | "deno test"
        | "node --test" => js_filters(args),
        "cargo test" => cargo_filters(false, args),
        "cargo nextest" => cargo_filters(true, args),
        "go test" => go_filters(args),
        "make test" => make_filters(args),
        "nx test" => nx_filters(args),
        _ => other_filters(runner, args),
    }
}

/// The filters of a test script of `package.json`. The package manager gives the
/// script the words after its name, but npm reads the options before `--` itself.
/// The project or the workspace of the manager must be the same in two runs.
fn script_filters(script: &str, words: &[String]) -> Vec<Filter> {
    let Some(m) = words
        .iter()
        .position(|w| matches!(base_name(w).as_str(), "npm" | "pnpm" | "yarn" | "bun"))
    else {
        return Vec::new();
    };
    let manager = base_name(&words[m]);
    let mut out = Vec::new();
    // The options of the manager before the subcommand.
    let mut i = m + 1;
    while let Some(w) = words.get(i) {
        let (flag, inline) = split_option(w);
        let mut value = || {
            inline
                .map(str::to_string)
                .or_else(|| {
                    i += 1;
                    words.get(i).cloned()
                })
                .unwrap_or_default()
        };
        let place = |text: String| Filter::new(Role::Place, text);
        match (manager.as_str(), flag) {
            // The folder of the project: see `folder_options`.
            (_, "--prefix" | "-C" | "--dir" | "--cwd") => {
                value();
            }
            (_, "--filter" | "-F") => out.push(place(format!("--filter {}", value()))),
            ("npm", "-w" | "--workspace") => out.push(place(format!("--workspace {}", value()))),
            ("yarn", "workspace") => out.push(place(format!("workspace {}", value()))),
            (_, "-w" | "--workspace-root" | "-r" | "--recursive" | "--workspaces" | "-ws") => {
                out.push(place(flag.to_string()));
            }
            (_, f) if f.starts_with('-') && f != "--" => {}
            _ => break,
        }
        i += 1;
    }
    let is_name =
        |w: &String| w == script || (script == "test" && matches!(w.as_str(), "t" | "tst"));
    let start = i.min(words.len());
    let Some(p) = words[start..].iter().position(is_name) else {
        return out;
    };
    let rest = &words[start + p + 1..];
    if manager != "npm" {
        let args: Vec<String> = rest.iter().filter(|w| *w != "--").cloned().collect();
        out.extend(js_filters(&args));
        return out;
    }
    // npm reads the options before `--` itself, also the workspace options.
    const NPM: Options = &[
        ("-w", Opt::Filter(Role::Place, "--workspace")),
        ("--workspace", Opt::Filter(Role::Place, "--workspace")),
        ("-ws", Opt::Flag(Role::Place, "--workspaces")),
        ("--workspaces", Opt::Flag(Role::Place, "--workspaces")),
        ("--prefix", Opt::Value),
        ("-C", Opt::Value),
    ];
    let (before, after) = split_at_dashes(rest);
    let mut args = Vec::new();
    out.extend(read(before, &[NPM], |w, _| args.push(w.to_string())));
    args.extend(after.iter().cloned());
    out.extend(js_filters(&args));
    out
}

/// Filters of pytest and nose.
fn pytest_filters(args: &[String]) -> Vec<Filter> {
    use Role::Only;
    const OPTIONS: Options = &[
        ("-k", Opt::Filter(Only, "-k")),
        ("-m", Opt::Filter(Only, "-m")),
        ("--deselect", Opt::Filter(Only, "--deselect")),
        ("--ignore", Opt::Filter(Only, "--ignore")),
        ("--ignore-glob", Opt::Filter(Only, "--ignore-glob")),
        ("--lf", Opt::Flag(Only, "--lf")),
        ("--last-failed", Opt::Flag(Only, "--lf")),
        ("--sw", Opt::Flag(Only, "--sw")),
        ("--stepwise", Opt::Flag(Only, "--sw")),
        ("-p", Opt::Value),
        ("-c", Opt::Value),
        ("--config-file", Opt::Value),
        ("-n", Opt::Value),
        ("--numprocesses", Opt::Value),
        ("--dist", Opt::Value),
        ("-r", Opt::Value),
        ("-W", Opt::Value),
        ("-o", Opt::Value),
        ("--override-ini", Opt::Value),
        ("--maxfail", Opt::Value),
        ("--timeout", Opt::Value),
        ("--rootdir", Opt::Value),
        ("--cov", Opt::Value),
        ("--cov-report", Opt::Value),
        ("--cov-fail-under", Opt::Value),
        ("--junitxml", Opt::Value),
        ("--junit-xml", Opt::Value),
        ("--html", Opt::Value),
        ("--durations", Opt::Value),
        ("--basetemp", Opt::Value),
        ("--log-level", Opt::Value),
        ("--log-cli-level", Opt::Value),
        ("--log-file", Opt::Value),
        ("--reruns", Opt::Value),
        ("--count", Opt::Value),
        ("--confcutdir", Opt::Value),
        ("--import-mode", Opt::Value),
        ("--tb", Opt::Value),
        ("--color", Opt::Value),
        ("--capture", Opt::Value),
    ];
    read(args, &[OPTIONS], |w, out| {
        if looks_like_path(w) {
            push_path(w, out);
        }
    })
}

/// Filters of `python -m unittest`. After `discover`, the words are the start
/// folder, the file pattern and the top folder.
fn unittest_filters(args: &[String]) -> Vec<Filter> {
    use Role::Only;
    const OPTIONS: Options = &[
        ("-k", Opt::Filter(Only, "-k")),
        ("-p", Opt::Filter(Only, "-p")),
        ("--pattern", Opt::Filter(Only, "-p")),
        ("-s", Opt::Path),
        ("--start-directory", Opt::Path),
        ("-t", Opt::Value),
        ("--top-level-directory", Opt::Value),
        ("--durations", Opt::Value),
    ];
    let mut discover: Option<usize> = None;
    read(args, &[OPTIONS], |w, out| match discover {
        None if w == "discover" => discover = Some(0),
        // A module, a class, a test or a file.
        None => push_path(w, out),
        Some(n) => {
            match n {
                0 => push_path(w, out),
                1 => out.push(Filter::new(Only, format!("-p {w}"))),
                _ => {}
            }
            discover = Some(n + 1);
        }
    })
}

/// Filters of `manage.py test`: the labels of apps, modules and tests.
fn django_filters(args: &[String]) -> Vec<Filter> {
    use Role::Only;
    const OPTIONS: Options = &[
        ("-k", Opt::Filter(Only, "-k")),
        ("--tag", Opt::Filter(Only, "--tag")),
        ("--exclude-tag", Opt::Filter(Only, "--exclude-tag")),
        ("-p", Opt::Filter(Only, "-p")),
        ("--pattern", Opt::Filter(Only, "-p")),
        ("-v", Opt::Value),
        ("--verbosity", Opt::Value),
        ("--settings", Opt::Value),
        ("--testrunner", Opt::Value),
        ("--pythonpath", Opt::Value),
        ("-t", Opt::Value),
        ("--top-level-directory", Opt::Value),
        ("--parallel", Opt::Count),
        ("--shuffle", Opt::Count),
    ];
    read(args, &[OPTIONS], push_path)
}

/// Filters of JavaScript runners such as jest, vitest, mocha and playwright.
fn js_filters(args: &[String]) -> Vec<Filter> {
    use Role::{Only, Place, Scope};
    const OPTIONS: Options = &[
        ("-t", Opt::Filter(Only, "-t")),
        ("--testNamePattern", Opt::Filter(Only, "-t")),
        ("--test-name-pattern", Opt::Filter(Only, "-t")),
        ("-g", Opt::Filter(Only, "--grep")),
        ("--grep", Opt::Filter(Only, "--grep")),
        ("--fgrep", Opt::Filter(Only, "--fgrep")),
        ("--grep-invert", Opt::Filter(Only, "--grep-invert")),
        ("--invert", Opt::Flag(Only, "--invert")),
        ("--match", Opt::Filter(Only, "--match")),
        ("--filter", Opt::Filter(Only, "--filter")),
        (
            "--test-skip-pattern",
            Opt::Filter(Only, "--test-skip-pattern"),
        ),
        ("--test-only", Opt::Flag(Only, "--test-only")),
        (
            "--testPathIgnorePatterns",
            Opt::Filter(Only, "--testPathIgnorePatterns"),
        ),
        ("--shard", Opt::Filter(Only, "--shard")),
        ("--test-shard", Opt::Filter(Only, "--shard")),
        ("--changedSince", Opt::Filter(Only, "--changedSince")),
        ("-o", Opt::Flag(Only, "--onlyChanged")),
        ("--onlyChanged", Opt::Flag(Only, "--onlyChanged")),
        ("--changed", Opt::Flag(Only, "--changed")),
        ("--findRelatedTests", Opt::Flag(Only, "--findRelatedTests")),
        ("--related", Opt::Flag(Only, "--findRelatedTests")),
        ("--lastFailed", Opt::Flag(Only, "--onlyFailures")),
        ("--last-failed", Opt::Flag(Only, "--onlyFailures")),
        ("--onlyFailures", Opt::Flag(Only, "--onlyFailures")),
        ("--only-failures", Opt::Flag(Only, "--onlyFailures")),
        ("--ignoreProjects", Opt::Filter(Only, "--ignoreProjects")),
        ("--testPathPattern", Opt::Path),
        ("--testPathPatterns", Opt::Path),
        ("--spec", Opt::Path),
        ("--project", Opt::Filter(Scope("--project"), "--project")),
        (
            "--selectProjects",
            Opt::Filter(Scope("--project"), "--project"),
        ),
        ("-c", Opt::Filter(Place, "--config")),
        ("--config", Opt::Filter(Place, "--config")),
        ("--root", Opt::Filter(Place, "--root")),
        ("--dir", Opt::Filter(Place, "--root")),
        ("--rootDir", Opt::Filter(Place, "--root")),
        ("--bail", Opt::Count),
        ("--reporter", Opt::Value),
        ("--reporters", Opt::Value),
        ("--reporter-options", Opt::Value),
        ("--maxWorkers", Opt::Value),
        ("-w", Opt::Value),
        ("--testTimeout", Opt::Value),
        ("--timeout", Opt::Value),
        ("--coverageReporters", Opt::Value),
        ("--coverageDirectory", Opt::Value),
        ("--outputFile", Opt::Value),
        ("--output", Opt::Value),
        ("--pool", Opt::Value),
        ("--environment", Opt::Value),
        ("--env", Opt::Value),
        ("--testEnvironment", Opt::Value),
        ("--mode", Opt::Value),
        ("--workers", Opt::Value),
        ("-j", Opt::Value),
        ("--retries", Opt::Value),
        ("--retry", Opt::Value),
        ("--repeat-each", Opt::Value),
        ("--trace", Opt::Value),
        ("--max-failures", Opt::Value),
        ("--global-timeout", Opt::Value),
        ("--import", Opt::Value),
        ("--require", Opt::Value),
        ("-r", Opt::Value),
        ("--loader", Opt::Value),
        ("--test-reporter", Opt::Value),
        ("--test-reporter-destination", Opt::Value),
        ("--test-concurrency", Opt::Value),
        ("--test-timeout", Opt::Value),
        ("--slow", Opt::Value),
        ("--extension", Opt::Value),
        ("--seed", Opt::Value),
        ("--import-map", Opt::Value),
        ("--location", Opt::Value),
        ("--concurrency", Opt::Value),
    ];
    read(args, &[OPTIONS], |w, out| {
        if !matches!(w, "run" | "watch" | "dev" | "related") && norm_path(w) != "src" {
            push_path(w, out);
        }
    })
}

/// The targets, packages and other options of cargo and nextest before `--`.
const CARGO: Options = &[
    ("--lib", Opt::Flag(Role::Scope("target"), "--lib")),
    ("--bins", Opt::Flag(Role::Scope("target"), "--bins")),
    ("--tests", Opt::Flag(Role::Scope("target"), "--tests")),
    ("--doc", Opt::Flag(Role::Scope("target"), "--doc")),
    ("--examples", Opt::Flag(Role::Scope("target"), "--examples")),
    ("--benches", Opt::Flag(Role::Scope("target"), "--benches")),
    ("--bin", Opt::Filter(Role::Scope("target"), "--bin")),
    ("--test", Opt::Filter(Role::Scope("target"), "--test")),
    ("--example", Opt::Filter(Role::Scope("target"), "--example")),
    ("--bench", Opt::Filter(Role::Scope("target"), "--bench")),
    ("-p", Opt::Filter(Role::Scope("-p"), "-p")),
    ("--package", Opt::Filter(Role::Scope("-p"), "-p")),
    ("--exclude", Opt::Filter(Role::Only, "--exclude")),
    ("--features", Opt::Value),
    ("-F", Opt::Value),
    ("--target", Opt::Value),
    ("-j", Opt::Value),
    ("--jobs", Opt::Value),
    ("--manifest-path", Opt::Value),
    ("--profile", Opt::Value),
    ("--color", Opt::Value),
    ("--message-format", Opt::Value),
    ("-Z", Opt::Value),
    ("--target-dir", Opt::Value),
    ("--config", Opt::Value),
];

/// The options of the test program after `--`.
const HARNESS: Options = &[
    ("--skip", Opt::Filter(Role::Only, "--skip")),
    ("--ignored", Opt::Flag(Role::Place, "--ignored")),
    ("--exact", Opt::Flag(Role::Place, "--exact")),
    ("--test-threads", Opt::Value),
    ("--format", Opt::Value),
    ("--logfile", Opt::Value),
    ("--color", Opt::Value),
    ("-Z", Opt::Value),
    ("--shuffle-seed", Opt::Value),
];

/// Filters of `cargo test` and `cargo nextest run`: targets, packages and test names.
fn cargo_filters(nextest: bool, args: &[String]) -> Vec<Filter> {
    use Role::{Only, Place};
    const NEXTEST: Options = &[
        ("-E", Opt::Filter(Only, "-E")),
        ("--filterset", Opt::Filter(Only, "-E")),
        ("--filter-expr", Opt::Filter(Only, "-E")),
        ("--partition", Opt::Filter(Only, "--partition")),
        ("--run-ignored", Opt::Filter(Place, "--run-ignored")),
        ("-P", Opt::Value),
        ("--test-threads", Opt::Value),
        ("--retries", Opt::Value),
        ("--failure-output", Opt::Value),
        ("--success-output", Opt::Value),
        ("--status-level", Opt::Value),
        ("--final-status-level", Opt::Value),
        ("--cargo-profile", Opt::Value),
    ];
    let name = |w: &str, out: &mut Vec<Filter>| out.push(Filter::new(Role::Name, w));
    let (before, after) = split_at_dashes(args);
    let tables: &[Options] = if nextest { &[CARGO, NEXTEST] } else { &[CARGO] };
    let mut out = read(before, tables, |w, out| {
        // `run` is the subcommand of nextest, not a test name.
        if !(nextest && matches!(w, "run" | "r")) {
            name(w, out);
        }
    });
    out.extend(read(after, &[HARNESS], name));
    out
}

/// Filters of `go test`. `./...` selects all packages. With no package, go tests
/// only the package in the current folder. A package with `/...` holds the
/// packages in its folders.
fn go_filters(args: &[String]) -> Vec<Filter> {
    use Role::{Only, Place, Scope};
    const OPTIONS: Options = &[
        ("-run", Opt::Filter(Only, "-run")),
        ("-skip", Opt::Filter(Only, "-skip")),
        ("-short", Opt::Flag(Only, "-short")),
        ("-tags", Opt::Filter(Place, "-tags")),
        ("-count", Opt::Value),
        ("-timeout", Opt::Value),
        ("-p", Opt::Value),
        ("-parallel", Opt::Value),
        ("-coverprofile", Opt::Value),
        ("-covermode", Opt::Value),
        ("-coverpkg", Opt::Value),
        ("-o", Opt::Value),
        ("-cpu", Opt::Value),
        ("-benchtime", Opt::Value),
        ("-bench", Opt::Value),
        ("-ldflags", Opt::Value),
        ("-gcflags", Opt::Value),
        ("-asmflags", Opt::Value),
        ("-mod", Opt::Value),
        ("-modfile", Opt::Value),
        ("-C", Opt::Value),
        ("-exec", Opt::Value),
        ("-vet", Opt::Value),
        ("-shuffle", Opt::Value),
        ("-fuzz", Opt::Value),
        ("-fuzztime", Opt::Value),
        ("-outputdir", Opt::Value),
        ("-cpuprofile", Opt::Value),
        ("-memprofile", Opt::Value),
        ("-blockprofile", Opt::Value),
        ("-mutexprofile", Opt::Value),
        ("-trace", Opt::Value),
        ("-toolexec", Opt::Value),
        ("-overlay", Opt::Value),
    ];
    // go also reads `--run` as `-run`. The words after `-args` go to the test program.
    let args: Vec<String> = args
        .iter()
        .take_while(|a| !matches!(a.as_str(), "-args" | "--args"))
        .map(|a| {
            a.strip_prefix('-')
                .filter(|rest| rest.starts_with('-'))
                .unwrap_or(a)
                .to_string()
        })
        .collect();
    let mut all = false;
    let mut out = read(&args, &[OPTIONS], |w, out| match norm_path(w).as_str() {
        "..." => all = true,
        "" => out.push(Filter::new(Scope(""), ".")),
        p => out.push(Filter::new(Scope(""), p)),
    });
    let packages = out.iter().any(|f| f.role == Scope(""));
    if all {
        out.retain(|f| f.role != Scope(""));
    } else if !packages {
        out.push(Filter::new(Scope(""), "."));
    }
    out
}

/// Filters of `make`: the targets other than `test`, `tests`, `check` and the usual
/// build targets, and variables such as `TESTS=tests/test_a.py`.
fn make_filters(args: &[String]) -> Vec<Filter> {
    const OPTIONS: Options = &[
        ("-f", Opt::Filter(Role::Place, "-f")),
        ("--file", Opt::Filter(Role::Place, "-f")),
        ("--makefile", Opt::Filter(Role::Place, "-f")),
        ("-C", Opt::Value),
        ("--directory", Opt::Value),
        ("-I", Opt::Value),
        ("--include-dir", Opt::Value),
        ("-o", Opt::Value),
        ("-W", Opt::Value),
        ("-j", Opt::Count),
        ("--jobs", Opt::Count),
        ("-l", Opt::Count),
        ("--load-average", Opt::Count),
    ];
    read(args, &[OPTIONS], |w, out| {
        if w.contains('=') {
            out.push(Filter::new(Role::Only, w));
        } else if !matches!(
            w,
            "test"
                | "tests"
                | "check"
                | "clean"
                | "build"
                | "all"
                | "install"
                | "deps"
                | "setup"
                | "lint"
                | "fmt"
                | "format"
                | "vet"
        ) {
            out.push(Filter::new(Role::Scope("task"), w));
        }
    })
}

/// Filters of nx: `nx test web`, `nx run web:test`, `nx run-many -t test -p web,api`
/// and `nx affected -t test`.
fn nx_filters(args: &[String]) -> Vec<Filter> {
    use Role::{Only, Place, Scope};
    const OPTIONS: Options = &[
        ("--projects", Opt::List(Scope("project"), "")),
        ("-p", Opt::List(Scope("project"), "")),
        ("--exclude", Opt::Filter(Only, "--exclude")),
        ("-c", Opt::Filter(Place, "--configuration")),
        ("--configuration", Opt::Filter(Place, "--configuration")),
        ("--testFile", Opt::Path),
        ("--test-file", Opt::Path),
        ("--testNamePattern", Opt::Filter(Only, "-t")),
        ("-t", Opt::Value),
        ("--target", Opt::Value),
        ("--targets", Opt::Value),
        ("--parallel", Opt::Count),
        ("--base", Opt::Value),
        ("--head", Opt::Value),
        ("--output-style", Opt::Value),
    ];
    // The nx command, then the project.
    let mut command: Option<String> = None;
    read(args, &[OPTIONS], |w, out| {
        match command.as_deref() {
            None => {
                if w == "affected" {
                    out.push(Filter::new(Only, "affected"));
                }
                command = Some(w.to_string());
                return;
            }
            Some("run") => out.push(Filter::new(
                Scope("project"),
                w.split(':').next().unwrap_or(w),
            )),
            Some("run-many" | "affected" | "") => {}
            Some(_) => out.push(Filter::new(Scope("project"), w)),
        }
        command = Some(String::new());
    })
}

/// Filters of other runners.
fn other_filters(runner: &str, args: &[String]) -> Vec<Filter> {
    use Role::{Only, Place, Scope};
    const MAVEN: Options = &[
        ("-Dtest", Opt::Filter(Only, "-Dtest")),
        ("-Dit.test", Opt::Filter(Only, "-Dit.test")),
        ("-Dgroups", Opt::Filter(Only, "-Dgroups")),
        ("-DexcludedGroups", Opt::Filter(Only, "-DexcludedGroups")),
        ("-rf", Opt::Filter(Only, "-rf")),
        ("--resume-from", Opt::Filter(Only, "-rf")),
        ("-pl", Opt::List(Scope("-pl"), "-pl")),
        ("--projects", Opt::List(Scope("-pl"), "-pl")),
        ("-P", Opt::Filter(Place, "-P")),
        ("--activate-profiles", Opt::Filter(Place, "-P")),
        ("-f", Opt::Value),
        ("--file", Opt::Value),
        ("-D", Opt::Value),
        ("-T", Opt::Value),
        ("-s", Opt::Value),
        ("-gs", Opt::Value),
    ];
    const GRADLE: Options = &[
        ("--tests", Opt::Filter(Scope("--tests"), "--tests")),
        ("-x", Opt::Filter(Only, "-x")),
        ("--exclude-task", Opt::Filter(Only, "-x")),
        ("-p", Opt::Value),
        ("--project-dir", Opt::Value),
        ("-P", Opt::Value),
        ("-D", Opt::Value),
        ("-c", Opt::Value),
        ("-b", Opt::Value),
        ("-I", Opt::Value),
        ("--console", Opt::Value),
        ("--max-workers", Opt::Value),
        ("--warning-mode", Opt::Value),
    ];
    const DOTNET: Options = &[
        ("--filter", Opt::Filter(Only, "--filter")),
        ("--project", Opt::Path),
        ("--solution", Opt::Path),
        ("-s", Opt::Filter(Place, "--settings")),
        ("--settings", Opt::Filter(Place, "--settings")),
        ("-c", Opt::Value),
        ("--configuration", Opt::Value),
        ("-f", Opt::Value),
        ("--framework", Opt::Value),
        ("-l", Opt::Value),
        ("--logger", Opt::Value),
        ("-v", Opt::Value),
        ("--verbosity", Opt::Value),
        ("-r", Opt::Value),
        ("--results-directory", Opt::Value),
        ("--collect", Opt::Value),
        ("-a", Opt::Value),
        ("--test-adapter-path", Opt::Value),
        ("-d", Opt::Value),
        ("--diag", Opt::Value),
        ("-o", Opt::Value),
        ("--output", Opt::Value),
        ("-e", Opt::Value),
        ("--environment", Opt::Value),
        ("--arch", Opt::Value),
        ("--os", Opt::Value),
        ("--runtime", Opt::Value),
        ("--blame-hang-timeout", Opt::Value),
    ];
    const RSPEC: Options = &[
        ("-e", Opt::Filter(Only, "--example")),
        ("--example", Opt::Filter(Only, "--example")),
        ("-E", Opt::Filter(Only, "--example-matches")),
        ("--example-matches", Opt::Filter(Only, "--example-matches")),
        ("-t", Opt::Filter(Only, "--tag")),
        ("--tag", Opt::Filter(Only, "--tag")),
        ("--only-failures", Opt::Flag(Only, "--only-failures")),
        ("-n", Opt::Flag(Only, "--next-failure")),
        ("--next-failure", Opt::Flag(Only, "--next-failure")),
        ("-P", Opt::Filter(Only, "--pattern")),
        ("--pattern", Opt::Filter(Only, "--pattern")),
        ("--exclude-pattern", Opt::Filter(Only, "--exclude-pattern")),
        ("--default-path", Opt::Filter(Place, "--default-path")),
        ("-O", Opt::Filter(Place, "--options")),
        ("--options", Opt::Filter(Place, "--options")),
        ("-f", Opt::Value),
        ("--format", Opt::Value),
        ("-o", Opt::Value),
        ("--out", Opt::Value),
        ("-r", Opt::Value),
        ("--require", Opt::Value),
        ("-I", Opt::Value),
        ("--seed", Opt::Value),
        ("--order", Opt::Value),
        ("--profile", Opt::Count),
    ];
    const PHPUNIT: Options = &[
        ("--filter", Opt::Filter(Only, "--filter")),
        ("--group", Opt::Filter(Only, "--group")),
        ("--exclude-group", Opt::Filter(Only, "--exclude-group")),
        (
            "--exclude-testsuite",
            Opt::Filter(Only, "--exclude-testsuite"),
        ),
        (
            "--testsuite",
            Opt::List(Scope("--testsuite"), "--testsuite"),
        ),
        ("-c", Opt::Filter(Place, "--configuration")),
        ("--configuration", Opt::Filter(Place, "--configuration")),
        ("--bootstrap", Opt::Value),
        ("--log-junit", Opt::Value),
        ("--coverage-html", Opt::Value),
        ("--coverage-clover", Opt::Value),
        ("--coverage-xml", Opt::Value),
        ("--coverage-cobertura", Opt::Value),
        ("--coverage-php", Opt::Value),
        ("--order-by", Opt::Value),
        ("--cache-directory", Opt::Value),
        ("--random-order-seed", Opt::Value),
        ("--include-path", Opt::Value),
        ("-d", Opt::Value),
    ];
    const CTEST: Options = &[
        ("-R", Opt::Filter(Only, "-R")),
        ("--tests-regex", Opt::Filter(Only, "-R")),
        ("-E", Opt::Filter(Only, "-E")),
        ("--exclude-regex", Opt::Filter(Only, "-E")),
        ("-L", Opt::Filter(Only, "-L")),
        ("--label-regex", Opt::Filter(Only, "-L")),
        ("-LE", Opt::Filter(Only, "-LE")),
        ("--label-exclude", Opt::Filter(Only, "-LE")),
        ("-I", Opt::Filter(Only, "-I")),
        ("--tests-information", Opt::Filter(Only, "-I")),
        ("--rerun-failed", Opt::Flag(Only, "--rerun-failed")),
        ("--test-dir", Opt::Filter(Place, "--test-dir")),
        ("--preset", Opt::Filter(Place, "--preset")),
        ("-j", Opt::Count),
        ("--parallel", Opt::Count),
        ("-C", Opt::Value),
        ("--build-config", Opt::Value),
        ("--timeout", Opt::Value),
        ("--repeat", Opt::Value),
        ("--output-junit", Opt::Value),
        ("-O", Opt::Value),
        ("--output-log", Opt::Value),
        ("-T", Opt::Value),
        ("-D", Opt::Value),
        ("-S", Opt::Value),
    ];
    const BATS: Options = &[
        ("-f", Opt::Filter(Only, "--filter")),
        ("--filter", Opt::Filter(Only, "--filter")),
        ("--filter-tags", Opt::Filter(Only, "--filter-tags")),
        ("--filter-status", Opt::Filter(Only, "--filter-status")),
        ("-j", Opt::Value),
        ("--jobs", Opt::Value),
        ("-F", Opt::Value),
        ("--formatter", Opt::Value),
        ("-o", Opt::Value),
        ("--output", Opt::Value),
        ("--report-formatter", Opt::Value),
        ("--setup-suite-file", Opt::Value),
        ("--gather-test-outputs-in", Opt::Value),
    ];
    // tox and nox. The words after `--` go to pytest.
    const TOX: Options = &[
        ("-e", Opt::Filter(Only, "-e")),
        ("-m", Opt::Filter(Only, "-m")),
        ("-f", Opt::Filter(Only, "-f")),
        ("--skip-env", Opt::Filter(Only, "--skip-env")),
        ("-s", Opt::Filter(Only, "-s")),
        ("--session", Opt::Filter(Only, "-s")),
        ("--sessions", Opt::Filter(Only, "-s")),
        ("-k", Opt::Filter(Only, "-k")),
        ("-t", Opt::Filter(Only, "--tags")),
        ("--tags", Opt::Filter(Only, "--tags")),
        ("--python", Opt::Filter(Only, "--python")),
        ("-c", Opt::Filter(Place, "-c")),
        ("--root", Opt::Filter(Place, "--root")),
        ("--noxfile", Opt::Filter(Place, "--noxfile")),
        ("-p", Opt::Count),
        ("--parallel", Opt::Count),
        ("--workdir", Opt::Value),
        ("-x", Opt::Value),
        ("--override", Opt::Value),
    ];
    const BAZEL: Options = &[
        ("--test_filter", Opt::Filter(Only, "--test_filter")),
        (
            "--test_tag_filters",
            Opt::Filter(Only, "--test_tag_filters"),
        ),
        (
            "--test_lang_filters",
            Opt::Filter(Only, "--test_lang_filters"),
        ),
        (
            "--test_size_filters",
            Opt::Filter(Only, "--test_size_filters"),
        ),
        (
            "--test_timeout_filters",
            Opt::Filter(Only, "--test_timeout_filters"),
        ),
        ("--config", Opt::Filter(Place, "--config")),
        ("--test_output", Opt::Value),
        ("--test_arg", Opt::Value),
        ("--test_env", Opt::Value),
        ("--runs_per_test", Opt::Value),
        ("--test_timeout", Opt::Value),
        ("--jobs", Opt::Value),
        ("-j", Opt::Value),
        ("--define", Opt::Value),
    ];
    const MIX: Options = &[
        ("--only", Opt::Filter(Only, "--only")),
        ("--exclude", Opt::Filter(Only, "--exclude")),
        ("--include", Opt::Filter(Place, "--include")),
        ("--failed", Opt::Flag(Only, "--failed")),
        ("--stale", Opt::Flag(Only, "--stale")),
        ("--partitions", Opt::Value),
        ("--seed", Opt::Value),
        ("--max-failures", Opt::Value),
        ("--max-cases", Opt::Value),
        ("--timeout", Opt::Value),
        ("--slowest", Opt::Value),
        ("--formatter", Opt::Value),
    ];
    const TURBO: Options = &[
        ("--filter", Opt::Filter(Scope("--filter"), "--filter")),
        ("-F", Opt::Filter(Scope("--filter"), "--filter")),
        ("--affected", Opt::Flag(Only, "--affected")),
        ("--concurrency", Opt::Value),
        ("--cache-dir", Opt::Value),
        ("--cache", Opt::Value),
        ("--log-order", Opt::Value),
        ("--log-prefix", Opt::Value),
        ("--output-logs", Opt::Value),
        ("--env-mode", Opt::Value),
    ];
    const JUST: Options = &[
        ("-f", Opt::Filter(Place, "--justfile")),
        ("--justfile", Opt::Filter(Place, "--justfile")),
        ("-d", Opt::Filter(Place, "--working-directory")),
        (
            "--working-directory",
            Opt::Filter(Place, "--working-directory"),
        ),
    ];
    let none = |_: &str, _: &mut Vec<Filter>| {};
    match runner {
        // The goals and phases of maven select no tests.
        "maven test" => read(args, &[MAVEN], none),
        // The gradle tasks, for example `test`, `check` or `:api:test`.
        "gradle test" => read(args, &[GRADLE], |w, out| {
            if !matches!(w, "clean" | "build" | "assemble") {
                out.push(Filter::new(Scope("task"), w));
            }
        }),
        "dotnet test" => read(split_at_dashes(args).0, &[DOTNET], push_path),
        "rspec" => read(args, &[RSPEC], push_path),
        "phpunit" => read(args, &[PHPUNIT], push_path),
        "ctest" => read(args, &[CTEST], none),
        "bats" => read(args, &[BATS], push_path),
        "tox" => {
            let (before, after) = split_at_dashes(args);
            let mut out = read(before, &[TOX], none);
            out.extend(pytest_filters(after));
            out
        }
        "bazel test" => read(args, &[BAZEL], push_path),
        "mix test" => read(args, &[MIX], push_path),
        "turbo test" => read(split_at_dashes(args).0, &[TURBO], none),
        "just test" => read(args, &[JUST], push_path),
        _ => Vec::new(),
    }
}

// ---------- The folder of a run ----------

/// True if a command part only prepares the shell: `cd`, `export`, `source` or
/// assignments almost never fail and run no program.
fn is_setup(part: &str) -> bool {
    static SAFE: OnceLock<Regex> = OnceLock::new();
    SAFE.get_or_init(|| {
        re(r"^(?:cd|pushd|export|set|source|\.)(?:\s|$)|^(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s*)+$")
    })
    .is_match(part)
}

/// The folders that `cd` and `pushd` move to before part `i` of a command list,
/// in order. A move in `( ... )` ends at the `)`. None for a folder that is not
/// known, for example `cd $DIR`.
fn moves_before(
    lists: &[(String, Option<&'static str>)],
    kept: &[(String, Option<&'static str>)],
    i: usize,
) -> Vec<Option<String>> {
    static CD: OnceLock<Regex> = OnceLock::new();
    let cd = CD.get_or_init(|| {
        re(r#"(?:^|[({]\s*)(cd|pushd|popd|chdir|Set-Location|sl)(?:\s+(?:/d|-Path|-LiteralPath|-L|-P))?(?:\s+("[^"]*"|'[^']*'|[^\s()]+))?\s*$"#)
    });
    // Each move with the depth of the brackets around it.
    let mut moves: Vec<(i32, Option<String>)> = Vec::new();
    let mut depth = 0;
    for (k, (text, before)) in lists.iter().enumerate().take(i) {
        for c in text.chars() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    moves.retain(|(d, _)| *d <= depth);
                }
                _ => {}
            }
        }
        // After `||`, the part runs only if the part before it failed.
        if *before == Some("||") {
            continue;
        }
        let Some(c) = kept.get(k).and_then(|(t, _)| cd.captures(t.trim())) else {
            continue;
        };
        if &c[1] == "popd" {
            moves.pop();
            continue;
        }
        let target = c.get(2).map(|m| m.as_str().trim_matches(['"', '\'']));
        let known = target.filter(|t| {
            !t.is_empty() && *t != "-" && !t.starts_with('~') && !t.contains(['$', '`', '%'])
        });
        moves.push((depth, known.map(str::to_string)));
    }
    moves.into_iter().map(|(_, m)| m).collect()
}

/// The folders that options of the runner move to, for example `make -C engine`,
/// `npm --prefix web` or `cargo test --manifest-path engine/Cargo.toml`.
fn folder_options(runner: &str, words: &[String]) -> Vec<Option<String>> {
    let options: &[&str] = match runner {
        r if r.starts_with("script ") => &["--prefix", "-C", "--dir", "--cwd"],
        "make test" => &["-C", "--directory"],
        "go test" => &["-C"],
        "cargo test" | "cargo nextest" => &["--manifest-path"],
        "gradle test" => &["-p", "--project-dir"],
        "maven test" => &["-f", "--file"],
        _ => return Vec::new(),
    };
    let words = split_at_dashes(words).0;
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(w) = words.get(i) {
        let (flag, inline) = split_option(w);
        if options.contains(&flag) {
            let value = inline.map(str::to_string).or_else(|| {
                i += 1;
                words.get(i).cloned()
            });
            // A file such as `Cargo.toml` or `pom.xml` gives its folder.
            out.push(value.map(|v| {
                if v.ends_with(".toml") || v.ends_with(".xml") {
                    v.rsplit_once(['/', '\\'])
                        .map_or(".".to_string(), |(dir, _)| dir.to_string())
                } else {
                    v
                }
            }));
        }
        i += 1;
    }
    out
}

/// A folder in one form, so that `C:\x`, `C:/x` and `/c/x` from Git Bash are the same.
fn norm_dir(path: &str) -> String {
    let mut p = path.trim_matches(['"', '\'']).replace('\\', "/");
    let b = p.as_bytes();
    if b.len() >= 2 && b[0] == b'/' && b[1].is_ascii_alphabetic() && (b.len() == 2 || b[2] == b'/')
    {
        p = format!("{}:{}", &p[1..2], &p[2..]);
    }
    let windows = p.as_bytes().get(1) == Some(&b':');
    let mut parts: Vec<&str> = Vec::new();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    let out = if p.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    };
    if windows {
        out.to_lowercase()
    } else {
        out
    }
}

/// True for a folder such as `/repo`, `C:/repo` or `c:/repo`.
fn is_absolute(dir: &str) -> bool {
    dir.starts_with(['/', '\\']) || dir.as_bytes().get(1) == Some(&b':')
}

/// The folder of a run from the moves of its command: an absolute folder, or a
/// folder relative to the start folder of the shell. The transcript does not always
/// show the start folder: the shell stays in the folder of the last `cd`, but the
/// folder in the transcript can be older. None if the command does not move, or if
/// the folder is not known, for example `cd $DIR` or `cd ..` from the start folder.
fn resolve(moves: &[Option<String>]) -> Option<String> {
    let mut dir = String::new();
    for m in moves {
        let m = m.as_deref()?;
        dir = if is_absolute(m) || dir.is_empty() {
            m.to_string()
        } else {
            format!("{dir}/{m}")
        };
    }
    if dir.is_empty()
        || (!is_absolute(&dir) && dir.replace('\\', "/").split('/').any(|p| p == ".."))
    {
        return None;
    }
    Some(norm_dir(&dir))
}

/// A test command that only its output shows, for example `bash run-tests.sh` or
/// a container with tests as its command. The exit code proves nothing, because
/// the command does not show the runner. The filters are the test files and test
/// folders that a script gets, for example `tests/test_a.py` in
/// `./scripts/test.sh tests/test_a.py`. The folder is not known, because the
/// command does not show which part ran the tests.
pub fn output_command(runner: &str, command: &str) -> TestCommand {
    let kept = split_lists(&finish(&keep_quotes(&prepare(command))));
    let filters = kept
        .iter()
        .flat_map(|(p, _)| p.split('|'))
        .filter_map(|stage| script_paths(&clean_words(stage)))
        .find(|f| !f.is_empty())
        .unwrap_or_default();
    TestCommand {
        runner: runner.to_string(),
        pass_hidden: true,
        fail_hidden: true,
        filters,
        dir: None,
    }
}

// ---------- What the output says ----------

/// The start and the end of the output that the engine keeps for a test command.
pub const KEEP_HEAD: usize = 2 * 1024;
pub const KEEP_TAIL: usize = 8 * 1024;

/// The first 2 KB and the last 8 KB of an output.
pub fn keep(output: &str) -> String {
    if output.len() <= KEEP_HEAD + KEEP_TAIL {
        return output.to_string();
    }
    let mut head = KEEP_HEAD;
    while !output.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = output.len() - KEEP_TAIL;
    while !output.is_char_boundary(tail) {
        tail += 1;
    }
    format!(
        "{}\n[{} bytes not kept]\n{}",
        &output[..head],
        tail - head,
        &output[tail..]
    )
}

/// One kind of summary line.
#[derive(Clone, Copy)]
enum Kind {
    /// Jest: `Tests: 1 failed, 2 passed, 3 total`.
    JestTests,
    /// Jest: `Test Suites: 1 failed, 1 total`.
    JestSuites,
    /// Vitest: `Tests  1 failed | 2 passed (3)`.
    VitestTests,
    /// Vitest: `Test Files  1 failed | 1 passed (2)`.
    VitestFiles,
    /// pytest: `==== 1 failed, 2 passed in 0.12s ====` or `1 failed, 2 passed in 0.12s`.
    Pytest,
    /// cargo: `test result: FAILED. 1 passed; 1 failed; 0 ignored; ...`.
    Cargo,
    /// cargo nextest: `Summary [ 0.1s] 3 tests run: 2 passed, 1 failed, 0 skipped`.
    Nextest,
    /// go: `ok  pkg 0.01s` or `ok  pkg 0.01s [no tests to run]`.
    GoOk,
    /// mocha: `2 passing (12ms)`.
    MochaPassing,
    /// mocha: `1 failing`.
    MochaFailing,
    /// bun: `2 pass`.
    BunPass,
    /// bun: `1 fail`.
    BunFail,
    /// bun: `Ran 3 tests across 1 file.`.
    BunRan,
    /// node --test: `# pass 2`, `# fail 1`, `# tests 3` (also with `ℹ`).
    Node,
    /// unittest and Django: `Ran 3 tests in 0.01s`.
    UnittestRan,
    /// rspec: `3 examples, 1 failure`.
    Rspec,
    /// dotnet: `Failed!  - Failed: 1, Passed: 2, Skipped: 0, Total: 3`.
    Dotnet,
    /// deno: `FAILED | 2 passed | 1 failed (12ms)`.
    Deno,
    /// Maven: `Tests run: 3, Failures: 1, Errors: 0, Skipped: 0`.
    Maven,
    /// ctest: `75% tests passed, 1 tests failed out of 4`.
    Ctest,
    /// playwright: `2 passed (3.1s)`.
    PlaywrightPassed,
    /// playwright: `1 failed`.
    PlaywrightFailed,
    /// A line that shows a failure.
    Fail,
    /// cargo: `error: could not compile`. It counts only before the first test
    /// result, because a later command such as `cargo clippy` prints it too.
    CargoBuildFailed,
    /// A line of one failed test, for example `E   AssertionError` or `FAIL src/a.test.js`.
    FailedTest,
    /// A line that shows a pass.
    Pass,
    /// A line that shows that no tests ran.
    NoTests,
}

fn patterns() -> &'static [(Regex, Kind)] {
    static PATTERNS: OnceLock<Vec<(Regex, Kind)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        use Kind::*;
        [
            (r"^Tests:\s+.*\b\d+ total$", JestTests),
            (r"^Test Suites:\s+.*\b\d+ total$", JestSuites),
            (r"^Tests\s{2,}.*\(\d+\)$", VitestTests),
            (r"^Test Files\s{2,}.*\(\d+\)$", VitestFiles),
            (r"^=+ .*\bin \d+(?:\.\d+)?s\b.*=+$", Pytest),
            (r"^(?:\d+ (?:passed|failed|errors?|skipped|deselected|xfailed|xpassed|warnings?|rerun)(?:, )?)+ in \d+(?:\.\d+)?s\b", Pytest),
            (r"^test result: (?:ok|FAILED)\. \d+ passed; \d+ failed;", Cargo),
            (r"^Summary \[.*\]\s+\d+ tests? run:", Nextest),
            (r"^ok\s+\S+\s+(?:\d+(?:\.\d+)?s|\(cached\))", GoOk),
            (r"^\d+ passing \(", MochaPassing),
            (r"^\d+ failing$", MochaFailing),
            (r"^\d+ pass$", BunPass),
            (r"^\d+ fail$", BunFail),
            (r"^Ran \d+ tests? across \d+ files?", BunRan),
            (r"^(?:#|ℹ) (?:pass|fail|tests) \d+$", Node),
            (r"^Ran \d+ tests? in \d", UnittestRan),
            (r"^\d+ examples?, \d+ failures?", Rspec),
            (r"^(?:Passed|Failed)!\s+-\s+Failed:\s+\d+, Passed:\s+\d+", Dotnet),
            (r"^(?:ok|FAILED) \| \d+ passed(?: \(\d+ steps?\))? \| \d+ failed", Deno),
            (r"^(?:\[\w+\] )?Tests run: \d+, Failures: \d+, Errors: \d+", Maven),
            (r"^\d+% tests passed, \d+ tests? failed out of \d+", Ctest),
            (r"^\d+ passed \([\d.]+m?s\)$", PlaywrightPassed),
            (r"^\d+ failed$", PlaywrightFailed),
            (
                r"^(?:FAIL\s+\S+\s+(?:\d+(?:\.\d+)?s|\[build failed\]|\[setup failed\])|--- FAIL: |FAIL$|error: test failed, to rerun pass|error: \d+ targets? failed|FAILED \((?:failures|errors)=|FAILURES!$|ERRORS!$|Test Run Failed\.$|\[ERROR\] .*There (?:are|were) test failures|Errors\s{2,}\d+ errors?$)",
                Fail,
            ),
            (r"^error: could not compile", CargoBuildFailed),
            // The lines of one failed test show a failure also when a pipe removed
            // the summary. pytest:
            (
                r"^(?:=+ (?:FAILURES|ERRORS) =+$|(?:FAILED|ERROR) \S+\.py\b|E {3}|\S+\.py:\d+: \w*(?:Error|Exception)\b|.*stopping after \d+ failures?)",
                FailedTest,
            ),
            // pytest progress with F or E, for example `....FF [ 35%]`.
            (r"^[.sxXFE]*[FE][.sxXFE]*\s+\[\s*\d+%\]$", FailedTest),
            // unittest, jest and vitest, cargo.
            (r"^(?:FAIL|ERROR): test\w* \(", FailedTest),
            (r"^FAIL\s+\S+\.(?:[cm]?[jt]sx?|vue|svelte)\b", FailedTest),
            (r"^test \S+ \.\.\. FAILED$", FailedTest),
            (r"^(?:OK \(\d+ tests?, |Test Run Successful\.$)", Pass),
            (
                r"^(?:No tests found|No test files found|no tests ran in |collected 0 items|\?\s+\S+\s+\[no test files\]|No tests executed!|No tests were found|\d+ tests? collected in \d|ERROR: file or directory not found:)",
                NoTests,
            ),
        ]
        .into_iter()
        .map(|(p, k)| (re(p), k))
        .collect()
    })
}

/// The number before each word in a summary line, for example "2 failed".
fn count(line: &str, words: &[&str]) -> u64 {
    static NUMBER_WORD: OnceLock<Regex> = OnceLock::new();
    NUMBER_WORD
        .get_or_init(|| re(r"(\d+) ([a-z]+)"))
        .captures_iter(line)
        .filter(|c| words.contains(&&c[2]))
        .filter_map(|c| c[1].parse::<u64>().ok())
        .sum()
}

/// The number after a label, for example "Failed: 2".
fn after(line: &str, label: &str) -> u64 {
    line.split(label)
        .nth(1)
        .and_then(|rest| {
            rest.trim_start()
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}

/// A line of the output, and the runner that writes this kind of line.
type Found = (String, Option<&'static str>);

#[derive(Default)]
struct Tally {
    /// The runner that writes the kind of line that `line()` reads now.
    current: Option<&'static str>,
    /// The first summary line with a count of failed tests.
    failed: Option<Found>,
    /// The first other line that shows a failure, for example `--- FAIL: TestX`.
    failed_other: Option<Found>,
    /// The first line of one failed test.
    failed_test: Option<String>,
    passed: Option<Found>,
    no_tests: Option<Found>,
    /// cargo: the sums of passed and failed tests, and the last result line.
    cargo: Option<(u64, u64, String)>,
    /// unittest: the number of tests in the last `Ran N tests` line.
    unittest_ran: Option<u64>,
    /// A filter left out tests: `3 deselected` or `3 filtered out`.
    narrowed: bool,
}

/// The runner that writes a kind of summary line with counts.
fn family_of(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::JestTests | Kind::JestSuites => Some("jest"),
        Kind::VitestTests | Kind::VitestFiles => Some("vitest"),
        Kind::Pytest => Some("pytest"),
        Kind::Cargo => Some("cargo test"),
        Kind::Nextest => Some("cargo nextest"),
        Kind::GoOk => Some("go test"),
        Kind::MochaPassing | Kind::MochaFailing => Some("mocha"),
        Kind::BunPass | Kind::BunFail | Kind::BunRan => Some("bun test"),
        Kind::Node => Some("node --test"),
        Kind::UnittestRan => Some("unittest"),
        Kind::Rspec => Some("rspec"),
        Kind::Dotnet => Some("dotnet test"),
        Kind::Deno => Some("deno test"),
        Kind::Maven => Some("maven test"),
        Kind::Ctest => Some("ctest"),
        Kind::PlaywrightPassed | Kind::PlaywrightFailed => Some("playwright test"),
        Kind::Fail | Kind::CargoBuildFailed | Kind::FailedTest | Kind::Pass | Kind::NoTests => None,
    }
}

impl Tally {
    fn fail(&mut self, line: &str) {
        let family = self.current;
        self.failed
            .get_or_insert_with(|| (line.to_string(), family));
    }
    fn fail_other(&mut self, line: &str) {
        let family = self.current;
        self.failed_other
            .get_or_insert_with(|| (line.to_string(), family));
    }
    fn pass(&mut self, line: &str) {
        self.passed = Some((line.to_string(), self.current));
    }
    fn none(&mut self, line: &str) {
        self.no_tests = Some((line.to_string(), self.current));
    }
    /// Passed, failed and other counts from one line.
    fn counts(&mut self, line: &str, failed: u64, passed: u64) {
        if failed > 0 {
            self.fail(line);
        } else if passed > 0 {
            self.pass(line);
        } else {
            self.none(line);
        }
    }

    fn line(&mut self, line: &str) {
        let Some((_, kind)) = patterns().iter().find(|(r, _)| r.is_match(line)) else {
            if line == "OK" || line.starts_with("OK (") {
                // unittest prints `OK` after `Ran N tests`.
                if self.unittest_ran.is_some_and(|n| n > 0) {
                    self.current = Some("unittest");
                    self.pass(line);
                }
            }
            return;
        };
        self.current = family_of(*kind);
        match kind {
            Kind::JestTests | Kind::VitestTests => {
                let failed = count(line, &["failed"]);
                let passed = count(line, &["passed"]);
                self.counts(line, failed, passed);
            }
            Kind::JestSuites | Kind::VitestFiles => {
                if count(line, &["failed"]) > 0 {
                    self.fail_other(line);
                }
            }
            Kind::Pytest => {
                let failed = count(line, &["failed", "error", "errors"]);
                let passed = count(line, &["passed", "xpassed"]);
                self.narrowed |= count(line, &["deselected"]) > 0;
                self.counts(line, failed, passed);
            }
            Kind::Cargo => {
                let failed = count(line, &["failed"]);
                let passed = count(line, &["passed"]);
                self.narrowed |= count(line, &["filtered"]) > 0;
                if line.contains("FAILED") || failed > 0 {
                    self.fail(line);
                }
                let sums = self.cargo.get_or_insert((0, 0, String::new()));
                sums.0 += passed;
                sums.1 += failed;
                sums.2 = line.to_string();
            }
            Kind::Nextest => {
                // "1 timed out" and "1 exec failed" are failures too.
                let failed = count(line, &["failed", "timed", "exec"]);
                let passed = count(line, &["passed"]);
                self.counts(line, failed, passed);
            }
            Kind::GoOk => {
                if line.contains("[no tests to run]") {
                    self.none(line);
                } else {
                    self.pass(line);
                }
            }
            Kind::MochaPassing | Kind::BunPass | Kind::PlaywrightPassed => {
                if count(line, &["passing", "pass", "passed"]) > 0 {
                    self.pass(line);
                } else {
                    self.none(line);
                }
            }
            Kind::MochaFailing | Kind::BunFail | Kind::PlaywrightFailed => {
                if count(line, &["failing", "fail", "failed"]) > 0 {
                    self.fail(line);
                }
            }
            Kind::BunRan => {
                if count(line, &["tests", "test"]) == 0 {
                    self.none(line);
                }
            }
            Kind::Node => {
                let mut words = line.split_whitespace().skip(1);
                let word = words.next().unwrap_or("");
                let n = words
                    .next()
                    .and_then(|n| n.parse::<u64>().ok())
                    .unwrap_or(0);
                match word {
                    "fail" if n > 0 => self.fail(line),
                    "pass" if n > 0 => self.pass(line),
                    "tests" if n == 0 => self.none(line),
                    _ => {}
                }
            }
            Kind::UnittestRan => {
                let n = count(line, &["tests", "test"]);
                self.unittest_ran = Some(n);
                if n == 0 {
                    self.none(line);
                }
            }
            Kind::Rspec => {
                // "1 error occurred outside of examples" is a failure too.
                let failed = count(line, &["failures", "failure", "errors", "error"]);
                let examples = count(line, &["examples", "example"]);
                self.counts(line, failed, examples.saturating_sub(failed));
            }
            Kind::Dotnet => {
                let failed = after(line, "Failed:");
                let passed = after(line, "Passed:");
                self.counts(line, failed, passed);
            }
            Kind::Deno => {
                let failed = count(line, &["failed"]);
                let passed = count(line, &["passed"]);
                self.counts(line, failed, passed);
            }
            Kind::Maven => {
                let failed = after(line, "Failures:") + after(line, "Errors:");
                let run = after(line, "Tests run:");
                self.counts(line, failed, run.saturating_sub(failed));
            }
            Kind::Ctest => {
                let failed = after(line, "passed,");
                let total = after(line, "out of");
                self.counts(line, failed, total.saturating_sub(failed));
            }
            Kind::Fail => self.fail_other(line),
            Kind::CargoBuildFailed => {
                if self.cargo.is_none() {
                    self.fail_other(line);
                }
            }
            Kind::FailedTest => {
                self.failed_test.get_or_insert_with(|| line.to_string());
            }
            Kind::Pass => self.pass(line),
            Kind::NoTests => self.none(line),
        }
    }

    fn finish(self) -> Option<Summary> {
        let narrowed = self.narrowed;
        let summary = |outcome: Outcome, (line, family): Found| Summary {
            outcome,
            line,
            family,
            narrowed,
        };
        // A summary line with counts is the best evidence, the line of one test the least.
        if let Some(found) = self.failed.or(self.failed_other) {
            return Some(summary(Outcome::Failed, found));
        }
        if let Some(line) = self.failed_test {
            return Some(summary(Outcome::Failed, (line, None)));
        }
        let cargo_passed = self.cargo.as_ref().is_some_and(|(p, _, _)| *p > 0);
        if let Some(found) = self.passed {
            return Some(summary(Outcome::Passed, found));
        }
        if let Some((_, _, line)) = self.cargo {
            let outcome = if cargo_passed {
                Outcome::Passed
            } else {
                Outcome::NoTests
            };
            return Some(summary(outcome, (line, Some("cargo test"))));
        }
        self.no_tests.map(|found| summary(Outcome::NoTests, found))
    }
}

/// Remove the color codes of a terminal.
fn plain(line: &str) -> std::borrow::Cow<'_, str> {
    static ANSI: OnceLock<Regex> = OnceLock::new();
    ANSI.get_or_init(|| re(r"\x1b\[[0-9;?]*[A-Za-z]"))
        .replace_all(line, "")
}

/// What the output of a test command says, if it has a summary.
pub fn summarize(output: &str) -> Option<Summary> {
    let mut tally = Tally::default();
    for raw in output.lines() {
        let line = plain(raw);
        let line = line.trim();
        if line.is_empty() || line.len() > 300 {
            continue;
        }
        tally.line(line);
    }
    tally.finish()
}

/// The summary line without run times, so that the same failure keeps the same text.
pub fn without_times(line: &str) -> String {
    static TIMES: OnceLock<Regex> = OnceLock::new();
    TIMES
        .get_or_init(|| {
            re(r"(?:\b(?:finished in|in)\s+)?\b\d+(?:\.\d+)?\s*m?s\b(?:\s*\(\d+(?::\d+)+\))?|\[\s*\d+(?:\.\d+)?\s*m?s\s*\]")
        })
        .replace_all(line, "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Two summaries of the same run, for example two reads of a background output.
/// A failure wins, then a pass, then "no tests".
pub fn merge(a: Option<Summary>, b: Option<Summary>) -> Option<Summary> {
    let rank = |s: &Summary| match s.outcome {
        Outcome::Failed => 3,
        Outcome::Passed => 2,
        Outcome::NoTests => 1,
        Outcome::Unknown => 0,
    };
    match (a, b) {
        (Some(a), Some(b)) => Some(if rank(&b) > rank(&a) { b } else { a }),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner(command: &str) -> Option<String> {
        detect(command).map(|t| t.runner)
    }

    #[test]
    fn finds_test_commands_in_many_forms() {
        for (command, expected) in [
            ("cd engine && cargo test -q", "cargo test"),
            ("cargo t", "cargo test"),
            ("cargo +nightly test --all", "cargo test"),
            ("cargo nextest run", "cargo nextest"),
            ("npx vitest run", "vitest"),
            ("npx jest 2>&1 | tail -20", "jest"),
            ("./node_modules/.bin/jest --ci", "jest"),
            ("python -m pytest tests/", "pytest"),
            ("uv run pytest -q", "pytest"),
            ("python manage.py test accounts", "django test"),
            ("npm test", "script test"),
            ("npm t", "script test"),
            ("npm run test", "script test"),
            ("npm run test:unit -- --watch=false", "script test:unit"),
            ("pnpm --filter web test", "script test"),
            ("yarn workspace api test", "script test"),
            ("bun run test", "script test"),
            ("bun test", "bun test"),
            ("nx test web", "nx test"),
            ("npx nx run web:test", "nx test"),
            ("turbo run test --filter=web", "turbo test"),
            ("just test", "just test"),
            ("bazel test //...", "bazel test"),
            ("go test ./...", "go test"),
            ("CI=true timeout 600 npm test", "script test"),
            (
                "docker compose run --rm dev sh -c \"cd engine && cargo test\"",
                "cargo test",
            ),
            (
                "docker compose --profile test run --rm -T backend-test pytest -q -p no:cacheprovider",
                "pytest",
            ),
            ("node --test test/", "node --test"),
            ("make -C engine test", "make test"),
            // Paths, versions and Windows forms.
            (".venv/bin/python -m pytest -q", "pytest"),
            (".venv/bin/pytest", "pytest"),
            ("python3.12 -m pytest", "pytest"),
            ("uvx pytest", "pytest"),
            ("yarn jest --ci", "jest"),
            ("pnpm vitest run", "vitest"),
            ("npx vitest@latest run", "vitest"),
            ("pnpm --dir web exec vitest run", "vitest"),
            ("make tests", "make test"),
            ("make test-unit", "make test"),
            (r".\gradlew test", "gradle test"),
            ("npm run -s test", "script test"),
            ("npm.cmd test", "script test"),
        ] {
            assert_eq!(runner(command).as_deref(), Some(expected), "{command}");
        }
    }

    #[test]
    fn other_commands_are_not_test_commands() {
        for command in [
            "cargo build",
            "cargo check",
            "cat jest.config.js",
            "echo protox",
            "grep -r jest src",
            "npm install --save-dev jest",
            "npm run build",
            "npm run lint",
            "git commit -m \"npm test\"",
            "docker exec app cat jest.config.js",
            "ls tests",
            "rm -rf test-results",
            "docker build -t app-test .",
            // Commands that build, list or explain the tests, but run none.
            "cargo test --no-run",
            "cargo test -- --list",
            "./gradlew build -x test",
            "mvn -DskipTests verify",
            "go test -c ./pkg",
            "npx jest --listTests",
            "npx vitest list",
            "npx playwright test --list",
            "pytest --collect-only",
            // The word "test" as an argument, not as the script.
            "npm run build -- --mode test",
            "yarn eslint src test",
            "yarn tsc -p test",
            "pnpm --filter test-utils build",
            "yarn sequelize db:migrate --env test",
        ] {
            assert_eq!(runner(command), None, "{command}");
        }
    }

    #[test]
    fn finds_shells_that_hide_the_exit_code() {
        let hidden = |c: &str| {
            let t = detect(c).unwrap();
            (t.pass_hidden, t.fail_hidden)
        };
        assert_eq!(hidden("cargo test"), (false, false));
        assert_eq!(hidden("cd engine && cargo test"), (false, false));
        assert_eq!(hidden("npx jest 2>&1 | tail -20"), (true, true));
        // With pipefail, exit code 0 proves a pass. A failure can come from `tee`.
        assert_eq!(
            hidden("set -o pipefail; npx jest | tee log.txt"),
            (false, true)
        );
        assert_eq!(hidden("cargo test && cargo clippy"), (false, true));
        assert_eq!(hidden("pytest; echo done"), (true, true));
        assert_eq!(hidden("npm test || true"), (true, true));
        assert_eq!(hidden("npm test &"), (true, false));
        assert_eq!(hidden("cargo test\necho done"), (true, true));
        // An apostrophe inside double quotes does not hide the later pipe and `;`.
        assert_eq!(
            hidden("npx jest -t \"user's profile\" 2>&1 | tail -n 3; echo 'done'"),
            (true, true)
        );
        // With pipefail, `grep` can still give the failure exit code.
        assert_eq!(
            hidden("set -o pipefail; pytest -q | grep FAILED"),
            (false, true)
        );
        assert_eq!(
            hidden("set +o pipefail; npx jest 2>&1 | tail -n 2"),
            (true, true)
        );
        // "pipefail" in a here-document is file content, not a setting.
        assert_eq!(
            hidden("cat > s.sh <<'EOF'\nset -euo pipefail\nEOF\nnpx jest 2>&1 | tail -n 2"),
            (true, true)
        );
        // When `npm ci` fails, the tests do not run.
        assert_eq!(hidden("npm ci && npm test"), (false, true));
        assert_eq!(hidden("! npx ava"), (true, true));
    }

    #[test]
    fn finds_the_filters_that_select_the_tests() {
        let filters = |c: &str| {
            detect(c)
                .unwrap()
                .filters
                .into_iter()
                .map(|f| f.text)
                .collect::<Vec<_>>()
        };
        let none: Vec<String> = Vec::new();
        let some = |f: &[&str]| f.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for (command, expected) in [
            ("npx jest", none.clone()),
            ("npx jest tests", none.clone()),
            ("npx jest --ci --watchAll=false", none.clone()),
            ("npx jest tests/math.test.js", some(&["tests/math.test.js"])),
            ("npx jest -t \"adds numbers\"", some(&["-t adds numbers"])),
            ("npx vitest run src/a.test.ts", some(&["src/a.test.ts"])),
            ("pytest -q", none.clone()),
            ("pytest tests/ -p no:cacheprovider --tb=short", none.clone()),
            ("pytest --color yes -q", none.clone()),
            (
                "pytest tests/test_a.py::test_x",
                some(&["tests/test_a.py::test_x"]),
            ),
            ("python -m pytest -k login", some(&["-k login"])),
            ("pytest --lf", some(&["--lf"])),
            ("python manage.py test accounts", some(&["accounts"])),
            ("cargo test", none.clone()),
            ("cargo test --release -- --test-threads 1", none.clone()),
            ("cargo test --lib", some(&["--lib"])),
            ("cargo test parser", some(&["parser"])),
            ("cargo test -p engine", some(&["-p engine"])),
            ("cargo test -- --ignored", some(&["--ignored"])),
            ("go test ./...", none.clone()),
            ("go test -run TestLogin ./...", some(&["-run TestLogin"])),
            ("go test ./pkg/...", some(&["pkg/..."])),
            ("go test", some(&["."])),
            ("go test ./pkg/auth", some(&["pkg/auth"])),
            ("npm test", none.clone()),
            ("npm test -- --watchAll=false", none.clone()),
            ("npm test -- tests/a.test.js", some(&["tests/a.test.js"])),
            ("yarn test tests/a.test.js", some(&["tests/a.test.js"])),
            ("make test", none.clone()),
            ("make test-unit", some(&["test-unit"])),
            (
                "make test TESTS=tests/test_a.py",
                some(&["TESTS=tests/test_a.py"]),
            ),
            // The file of a redirection is not a filter.
            ("pytest -q > /tmp/full_tests.log 2>&1", none.clone()),
            (
                "pytest -q 2>/tmp/err.log tests/test_a.py",
                some(&["tests/test_a.py"]),
            ),
            ("cargo test >> log.txt", none.clone()),
            ("cargo test &> out.txt", none.clone()),
            ("pytest -q &>> /tmp/pytest.log", none.clone()),
            (
                "docker compose --profile test run --rm web pytest -q tests/test_a.py",
                some(&["tests/test_a.py"]),
            ),
            // `-v` takes a value only in Django.
            ("pytest -v tests/test_a.py", some(&["tests/test_a.py"])),
            (
                "python -m unittest -v tests.test_auth",
                some(&["tests.test_auth"]),
            ),
            ("python manage.py test -v 2 blog", some(&["blog"])),
            // Words that are not filters.
            ("python -m unittest discover", none.clone()),
            ("python -m unittest discover -s tests -v", none.clone()),
            (
                "python -m unittest discover -s tests/unit",
                some(&["tests/unit"]),
            ),
            (
                "docker compose run --rm dev sh -c \"cd engine && cargo test\"",
                none.clone(),
            ),
            ("bash -c \"npx jest\"", none.clone()),
            ("(cd engine && cargo test parser)", some(&["parser"])),
            ("npx playwright test --trace on", none.clone()),
            ("npx vitest run --retry 2", none.clone()),
            ("npx playwright test --max-failures 1", none.clone()),
            ("node --test --import tsx", none.clone()),
            (
                "npx jest --bail tests/a.test.js",
                some(&["tests/a.test.js"]),
            ),
            (
                "pytest --cov src tests/test_a.py",
                some(&["tests/test_a.py"]),
            ),
            ("bazel test //...", none.clone()),
            ("bazel test //pkg:auth_test", some(&["//pkg:auth_test"])),
            // npm gives the words after the script name to the script.
            ("npm test src/auth.test.js", some(&["src/auth.test.js"])),
            ("npm run test:unit src/a.test.ts", some(&["src/a.test.ts"])),
            // The project of a package manager, a monorepo tool or maven.
            ("npm --prefix web test", none.clone()),
            ("npm test --workspace web", some(&["--workspace web"])),
            (
                "npm test -w web src/a.test.js",
                some(&["--workspace web", "src/a.test.js"]),
            ),
            ("pnpm --filter web test", some(&["--filter web"])),
            ("yarn workspace api test", some(&["workspace api"])),
            ("mvn -pl api,web test", some(&["-pl api", "-pl web"])),
            ("npx nx test web", some(&["web"])),
            ("go test ./pkg/auth/...", some(&["pkg/auth/..."])),
            ("cargo test --lib parser", some(&["--lib", "parser"])),
            ("cargo nextest run parser", some(&["parser"])),
            (
                "bundle exec rspec spec/models/user_spec.rb:42",
                some(&["spec/models/user_spec.rb:42"]),
            ),
            (
                "./gradlew test --tests com.x.LoginTest",
                some(&["test", "--tests com.x.LoginTest"]),
            ),
        ] {
            assert_eq!(filters(command), expected, "{command}");
        }
    }

    #[test]
    fn a_later_run_covers_an_earlier_run_only_with_the_same_tests_or_more() {
        let run = |command: &str| {
            let t = detect(command).unwrap_or_else(|| panic!("{command}"));
            decide(&t, Exit::Missing, None)
        };
        let covers_cmd = |later: &str, earlier: &str| covers(&run(later), &run(earlier));
        for (later, earlier) in [
            ("npx jest", "npx jest"),
            ("npx jest", "npx jest tests/math.test.js"),
            ("pytest tests/test_a.py", "pytest tests/test_a.py::test_x"),
            ("pytest tests/", "pytest"),
            ("cargo test parser", "cargo test parser"),
            // A later run without a name filter.
            (
                "pytest tests/test_auth.py",
                "pytest tests/test_auth.py -k test_login",
            ),
            ("go test ./pkg/dates", "go test ./pkg/dates -run TestParse"),
            ("go test -v", "go test -v -run TestParse"),
            ("cargo test --lib", "cargo test --lib parse_date"),
            (
                "npx jest tests/math.test.js",
                "npx jest tests/math.test.js -t adds",
            ),
            // A module, a file, a folder, a name or a pattern that holds the earlier tests.
            (
                "python manage.py test blog",
                "python manage.py test blog.tests.test_models.PostTests.test_slug",
            ),
            (
                "python -m unittest tests.test_auth",
                "python -m unittest tests.test_auth.TestLogin.test_ok",
            ),
            (
                "bundle exec rspec spec/models/user_spec.rb",
                "bundle exec rspec spec/models/user_spec.rb:42",
            ),
            (
                "npx playwright test tests/login.spec.ts",
                "npx playwright test tests/login.spec.ts:15",
            ),
            ("cargo test parse", "cargo test parse_empty_input"),
            ("cargo test --tests", "cargo test --test api"),
            ("pytest -k login", "pytest -k test_login_ok"),
            ("npx jest -t adds", "npx jest -t \"adds numbers\""),
            (
                "go test -run TestLogin ./...",
                "go test -run TestLoginOK ./...",
            ),
            ("npx jest auth", "npx jest src/auth/login.test.js"),
            (
                "pytest tests/test_game*.py",
                "pytest tests/test_game_review.py",
            ),
            (
                "pytest tests/test_game*.py",
                "pytest tests/test_game_review.py::test_x",
            ),
            ("go test ./pkg/auth/...", "go test ./pkg/auth"),
            ("bazel test //pkg/...", "bazel test //pkg/auth:test"),
            ("./gradlew test", "./gradlew :api:test"),
            ("./gradlew check", "./gradlew test"),
            ("mvn test", "mvn -pl api test"),
            ("npx nx run-many -t test", "npx nx test web"),
            // Words that are not filters.
            (
                "docker compose run --rm dev sh -c \"cd engine && cargo test\"",
                "docker compose run --rm dev sh -c \"cd engine && cargo test 2>&1 | tail -4\"",
            ),
            (
                "python -m unittest discover",
                "python -m unittest tests.test_auth",
            ),
            ("bazel test //...", "bazel test //pkg:auth_test"),
            (
                "npx playwright test --trace on",
                "npx playwright test tests/a.spec.ts",
            ),
            ("cargo test &> /tmp/cargo-test.log", "cargo test parser"),
        ] {
            assert!(covers_cmd(later, earlier), "{later} should cover {earlier}");
        }
        for (later, earlier) in [
            ("npx jest tests/math.test.js", "npx jest"),
            ("pytest tests/test_b.py", "pytest tests/test_a.py"),
            ("cargo test --lib", "cargo test"),
            ("go test", "go test ./..."),
            // A later run that adds a name filter.
            (
                "pytest tests/test_auth.py -k test_login",
                "pytest tests/test_auth.py",
            ),
            ("cargo test -p engine parse_date", "cargo test -p engine"),
            ("go test -run TestFormat", "go test"),
            (
                "npx jest tests/math.test.js -t adds",
                "npx jest tests/math.test.js",
            ),
            ("cargo test --lib parser", "cargo test --lib"),
            (
                "pytest tests/test_api.py -m smoke",
                "pytest tests/test_api.py",
            ),
            ("go test -run TestLogin ./pkg/...", "go test ./pkg/..."),
            ("pytest -k login", "pytest -k \"not login\""),
            (
                "pytest tests/test_game_review.py",
                "pytest tests/test_game*.py",
            ),
            ("pytest tests/test_*.py", "pytest tests/unit/test_a.py"),
            ("cargo test", "cargo test -- --ignored"),
            (
                "cargo test parse -- --exact",
                "cargo test parse_empty_input -- --exact",
            ),
            // `-v` before a path.
            ("pytest -v tests/test_math.py", "pytest"),
            // A smaller part of the tests or a different part.
            (
                "docker compose run --rm dev sh -c \"cd engine && cargo test parser\"",
                "docker compose run --rm dev sh -c \"cd engine && cargo test\"",
            ),
            ("npm test src/auth.test.js", "npm test"),
            ("pnpm --filter web test", "pnpm --filter api test"),
            ("npm --prefix web test", "npm --prefix api test"),
            ("make -C frontend test", "make -C backend test"),
            ("mvn -pl web test", "mvn -pl api test"),
            ("npx nx test web", "npx nx test api"),
            ("go test ./pkg/auth", "go test ./pkg/auth/..."),
            (
                "cargo test --manifest-path web/Cargo.toml",
                "cargo test --manifest-path api/Cargo.toml",
            ),
            (
                "npx vitest run --config vitest.unit.ts",
                "npx vitest run --config vitest.e2e.ts",
            ),
            ("cd web && npm test", "cd api && npm test"),
            ("cd /c/repo/backend && pytest", "cd /c/repo && pytest"),
        ] {
            assert!(
                !covers_cmd(later, earlier),
                "{later} should not cover {earlier}"
            );
        }
    }

    #[test]
    fn a_run_in_an_outer_folder_holds_the_tests_of_an_inner_folder() {
        let run = |command: &str| decide(&detect(command).unwrap(), Exit::Missing, None);
        let covers_cmd = |later: &str, earlier: &str| covers(&run(later), &run(earlier));
        // A run in the project folder can hold the tests of a folder in it.
        assert!(covers_cmd(
            "cd /repo && pytest",
            "cd /repo/backend && pytest"
        ));
        assert!(covers_cmd(
            "cd /repo && pytest backend/tests/test_a.py",
            "cd /repo/backend && pytest tests/test_a.py"
        ));
        assert!(!covers_cmd(
            "cd /repo && pytest backend/tests/test_b.py",
            "cd /repo/backend && pytest tests/test_a.py"
        ));
        // go: all packages in a folder.
        assert!(covers_cmd(
            "cd /repo && go test ./backend/...",
            "cd /repo/backend && go test ./..."
        ));
        assert!(!covers_cmd(
            "cd /repo && go test ./backend",
            "cd /repo/backend && go test ./..."
        ));
        // The shell stays in the folder of the last `cd`. Thus a later run with no
        // `cd` can be in the same folder, and the engine does not compare the folders.
        assert!(covers_cmd(
            "pytest tests/test_a.py",
            "cd backend && pytest tests/test_a.py"
        ));
        assert!(covers_cmd(
            "docker compose run --rm web pytest tests/test_a.py",
            "sed -i s/a/b/ app/x.py && cd .. && docker compose run --rm web pytest tests/test_a.py"
        ));
        assert!(covers_cmd(
            "cd /c/repo/backend && pytest tests/test_a.py",
            "cd backend && pytest tests/test_a.py"
        ));
    }

    #[test]
    fn finds_the_folder_of_a_run() {
        let dir = |command: &str| detect(command).unwrap().dir;
        let engine = Some("engine".to_string());
        assert_eq!(dir("cd engine && cargo test"), engine);
        assert_eq!(
            dir("docker compose run --rm dev sh -c \"cd engine && cargo test\""),
            engine
        );
        assert_eq!(dir("cargo test --manifest-path engine/Cargo.toml"), engine);
        assert_eq!(dir("npm --prefix engine test"), engine);
        assert_eq!(dir("make -C engine test"), engine);
        let absolute = Some("c:/users/x/repo/engine".to_string());
        assert_eq!(dir("cd /c/Users/x/repo/engine && cargo test"), absolute);
        assert_eq!(dir(r#"cd "C:\Users\x\repo\engine"; cargo test"#), absolute);
        assert_eq!(
            dir("cd /c/Users/x/repo/web && cd ../engine && cargo test"),
            absolute
        );
        // A `cd` in brackets ends at the `)`.
        assert_eq!(dir("(cd engine && cargo build) && cargo test"), None);
        // A folder that the command does not show.
        assert_eq!(dir("cargo test"), None);
        assert_eq!(dir("cd .. && cargo test"), None);
        assert_eq!(dir("cd $DIR && cargo test"), None);
        assert_eq!(dir("cd ~/repo && cargo test"), None);
    }

    #[test]
    fn a_run_from_the_output_has_the_test_files_of_its_command() {
        let filters = |command: &str| {
            output_command("pytest", command)
                .filters
                .into_iter()
                .map(|f| f.text)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            filters("./scripts/test.sh tests/test_dates.py"),
            ["tests/test_dates.py"]
        );
        assert_eq!(
            filters("cd backend && ./run_tests.sh tests/unit 2>&1 | tee log.txt"),
            ["tests/unit"]
        );
        assert!(filters("bash tests/run.sh").is_empty());
        assert!(filters("python tests/run_all.py").is_empty());
        assert!(filters("docker compose -f tests/compose.yaml run --rm app").is_empty());
        assert!(filters("./scripts/test.sh").is_empty());
        // `sed` edits a test file, but it does not select tests.
        assert!(filters(
            "cd /repo/backend && sed -i s/a/b/ tests/test_cli.py && python _t2.py && cd .. && docker compose run --rm backend-test"
        )
        .is_empty());
        // The command does not show which part ran the tests, so the folder is not known.
        assert_eq!(
            output_command("pytest", "cd backend && ./run_tests.sh").dir,
            None
        );
    }

    #[test]
    fn a_run_with_left_out_tests_covers_only_a_run_with_the_same_setting() {
        let run = |command: &str, line: &str| {
            let s = summarize(line);
            decide(&detect(command).unwrap(), Exit::Missing, s.as_ref())
        };
        let config_fail = run(
            "pytest",
            "===== 1 failed, 20 passed, 2 deselected in 1.20s =====",
        );
        let config_pass = run("pytest", "===== 21 passed, 2 deselected in 1.10s =====");
        let full_fail = run("pytest", "===== 1 failed, 22 passed in 1.20s =====");
        let marked_fail = run(
            "pytest -m slow",
            "===== 1 failed, 1 passed, 21 deselected in 1.20s =====",
        );
        // A setting of the project leaves out the same tests in each run.
        assert!(covers(&config_pass, &config_fail));
        assert!(!covers(&config_pass, &full_fail));
        assert!(!covers(&config_pass, &marked_fail));
        // A run of all tests whose output shows a filter does not cover.
        let mut narrowed = run(
            "cargo test",
            "test result: ok. 2 passed; 0 failed; 0 ignored;",
        );
        narrowed.narrowed = true;
        assert!(!covers(
            &narrowed,
            &run("cargo test", "test result: FAILED. 1 passed; 1 failed;")
        ));
    }

    #[test]
    fn filters_show_in_the_output() {
        let s = summarize("===== 5 passed, 12 deselected in 0.20s =====").unwrap();
        assert!(s.narrowed);
        let s =
            summarize("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out")
                .unwrap();
        assert!(s.narrowed);
        let s =
            summarize("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out")
                .unwrap();
        assert!(!s.narrowed);
    }

    #[test]
    fn run_times_are_not_part_of_the_evidence() {
        assert_eq!(
            without_times("===== 1 failed, 5 passed in 0.31s ====="),
            "===== 1 failed, 5 passed ====="
        );
        assert_eq!(
            without_times("test result: FAILED. 1 passed; 1 failed; finished in 0.49s"),
            "test result: FAILED. 1 passed; 1 failed;"
        );
        assert_eq!(
            without_times("Tests: 2 failed, 3 skipped, 5 total"),
            "Tests: 2 failed, 3 skipped, 5 total"
        );
    }

    #[test]
    fn reads_the_summaries_of_common_runners() {
        use Outcome::{Failed, NoTests, Passed};
        let cases = [
            // jest
            (
                "FAIL tests/math.test.js\nTests:       2 failed, 0 passed, 2 total",
                Some(Failed),
            ),
            ("Tests:       5 passed, 5 total\nTime: 1 s", Some(Passed)),
            ("Tests: 0 total", Some(NoTests)),
            ("No tests found, exiting with code 1", Some(NoTests)),
            (
                "Test Suites: 1 failed, 1 total\nTests:       0 total",
                Some(Failed),
            ),
            // vitest
            (
                " Test Files  1 failed | 2 passed (3)\n      Tests  2 failed | 10 passed (12)",
                Some(Failed),
            ),
            ("      Tests  12 passed (12)", Some(Passed)),
            // pytest
            ("===== 1 failed, 10 passed in 0.12s =====", Some(Failed)),
            ("10 passed, 2 warnings in 1.20s", Some(Passed)),
            ("===== no tests ran in 0.01s =====", Some(NoTests)),
            ("collected 0 items\n", Some(NoTests)),
            ("=== 1 error in 0.30s ===", Some(Failed)),
            // cargo
            (
                "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
                Some(Passed),
            ),
            (
                "test result: ok. 3 passed; 0 failed; 0 ignored;\ntest result: FAILED. 1 passed; 1 failed; 0 ignored;",
                Some(Failed),
            ),
            (
                "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out",
                Some(NoTests),
            ),
            (
                "error: could not compile `engine` (lib test) due to 2 previous errors",
                Some(Failed),
            ),
            // go
            (
                "ok  \texample.com/a\t0.012s\nFAIL\texample.com/b\t0.020s\nFAIL",
                Some(Failed),
            ),
            ("ok  \texample.com/a\t(cached)", Some(Passed)),
            ("?   \texample.com/c\t[no test files]", Some(NoTests)),
            ("ok  \texample.com/a\t0.01s [no tests to run]", Some(NoTests)),
            // other runners
            ("  3 passing (12ms)\n  1 failing", Some(Failed)),
            (
                " 4 pass\n 0 fail\nRan 4 tests across 1 file. [20.00ms]",
                Some(Passed),
            ),
            ("# tests 3\n# pass 2\n# fail 1", Some(Failed)),
            ("Ran 3 tests in 0.005s\n\nOK", Some(Passed)),
            ("Ran 3 tests in 0.005s\n\nFAILED (failures=1)", Some(Failed)),
            ("Ran 0 tests in 0.000s\n\nOK", Some(NoTests)),
            ("5 examples, 1 failure", Some(Failed)),
            (
                "Failed!  - Failed:     1, Passed:     4, Skipped:     0, Total:     5",
                Some(Failed),
            ),
            ("ok | 5 passed | 0 failed (12ms)", Some(Passed)),
            (
                "[ERROR] Tests run: 4, Failures: 1, Errors: 0, Skipped: 0",
                Some(Failed),
            ),
            ("OK (5 tests, 9 assertions)", Some(Passed)),
            (
                "\x1b[1m\x1b[31mTests:\x1b[39m\x1b[22m 1 failed, 1 total",
                Some(Failed),
            ),
            ("Compiling engine v0.1.0\nFinished `test` profile", None),
            // Failure lines without a summary, for example after `| grep` or `| head`.
            (
                "==== FAILURES ====\n___ test_x ___\nE   AssertionError: assert 1 == 2",
                Some(Failed),
            ),
            ("FAILED tests/test_a.py::test_b - AssertionError", Some(Failed)),
            ("ERROR tests/test_module_exercise.py", Some(Failed)),
            ("tests/test_documents.py:110: AssertionError", Some(Failed)),
            ("...............FFFF [ 35%]", Some(Failed)),
            ("!!!!!!!!!! stopping after 1 failures !!!!!!!!!!", Some(Failed)),
            ("FAIL src/math.test.ts > adds > negatives", Some(Failed)),
            ("test login_works ... FAILED", Some(Failed)),
            (
                "FAIL: test_add (tests.test_math.TestMath.test_add)",
                Some(Failed),
            ),
            ("88 tests collected in 23.23s", Some(NoTests)),
            (
                "ERROR: file or directory not found: tests/test_x.py",
                Some(NoTests),
            ),
            // Failures that a summary line gives with other words.
            (
                "0 examples, 0 failures, 1 error occurred outside of examples",
                Some(Failed),
            ),
            (
                " Test Files  1 passed (1)\n      Tests  3 passed (3)\n     Errors  1 error",
                Some(Failed),
            ),
            (
                "Summary [  60.012s] 3 tests run: 2 passed, 1 timed out, 0 skipped",
                Some(Failed),
            ),
            // A later `cargo clippy` that fails does not make the tests fail.
            (
                "test result: ok. 93 passed; 0 failed; 0 ignored;\nerror: could not compile `engine` (bin \"agentproof\") due to 1 previous error",
                Some(Passed),
            ),
            // No result: progress with no failure, and failure words of other tools.
            (".................................... [100%]", None),
            ("FAILED: Cycle is detected in revisions (a1, b2)", None),
            ("test login_works ... ok", None),
        ];
        for (output, expected) in cases {
            assert_eq!(summarize(output).map(|s| s.outcome), expected, "{output}");
        }
    }

    #[test]
    fn the_summary_comes_before_the_exit_code() {
        let piped = detect("npx jest 2>&1 | tail -20").unwrap();
        let plain = detect("npx jest").unwrap();
        let summary = |outcome, line: &str| Summary {
            outcome,
            line: line.into(),
            family: None,
            narrowed: false,
        };
        let failed = summary(Outcome::Failed, "Tests: 2 failed, 2 total");
        let passed = summary(Outcome::Passed, "Tests: 2 passed, 2 total");
        let none = summary(Outcome::NoTests, "Tests: 0 total");
        let ok = Exit::Ended { failed: false };
        let err = Exit::Ended { failed: true };
        let d = |c: &TestCommand, e: Exit, s: Option<&Summary>| {
            let r = decide(c, e, s);
            (r.outcome, r.basis)
        };
        // A pipe hides the exit code, but not the summary.
        assert_eq!(
            d(&piped, ok, Some(&failed)),
            (Outcome::Failed, Basis::Summary)
        );
        assert_eq!(
            d(&piped, ok, None),
            (Outcome::Unknown, Basis::HiddenExitCode)
        );
        assert_eq!(
            d(&piped, err, None),
            (Outcome::Unknown, Basis::HiddenExitCode)
        );
        assert_eq!(d(&plain, ok, None), (Outcome::Passed, Basis::ExitCode));
        assert_eq!(d(&plain, err, None), (Outcome::Failed, Basis::ExitCode));
        // The tests passed, but the runner failed, for example at a coverage limit.
        assert_eq!(
            d(&plain, err, Some(&passed)),
            (Outcome::Failed, Basis::ExitCode)
        );
        assert_eq!(
            d(&plain, err, Some(&none)),
            (Outcome::NoTests, Basis::Summary)
        );
        // A background run needs its summary.
        assert_eq!(
            d(&plain, Exit::Background(Some(0)), None),
            (Outcome::Unknown, Basis::Background)
        );
        assert_eq!(
            d(&plain, Exit::Background(Some(1)), None),
            (Outcome::Failed, Basis::ExitCode)
        );
        assert_eq!(
            d(&plain, Exit::Background(None), Some(&passed)),
            (Outcome::Passed, Basis::Summary)
        );
        // A coverage limit failed in the background, although all tests passed.
        assert_eq!(
            d(&plain, Exit::Background(Some(1)), Some(&passed)),
            (Outcome::Failed, Basis::ExitCode)
        );
        assert_eq!(
            d(&plain, Exit::Missing, None),
            (Outcome::Unknown, Basis::NoResult)
        );
        assert!(decide(&piped, ok, None).exit_code_hidden);
    }

    #[test]
    fn keeps_the_start_and_the_end_of_a_long_output() {
        let long = format!("{}{}", "a".repeat(5000), "z".repeat(20000));
        let kept = keep(&long);
        assert!(kept.starts_with(&"a".repeat(KEEP_HEAD)));
        assert!(kept.ends_with(&"z".repeat(KEEP_TAIL)));
        assert!(kept.len() < KEEP_HEAD + KEEP_TAIL + 100);
        assert_eq!(keep("short"), "short");
        let wide = "é".repeat(10_000);
        assert!(keep(&wide).ends_with('é'));
    }
}
