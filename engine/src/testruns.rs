//! Test commands and their results.
//!
//! A test command is a shell command that runs a test runner. The result of a
//! test run comes first from the summary that the runner writes in its output.
//! The exit code counts only when the shell cannot hide it, for example with a
//! pipe. The text of a tool result is what the agent saw: a result that is not
//! in the transcript was also not visible to the agent.

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
    }
}

// ---------- Which commands run tests ----------

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid pattern")
}

/// The runner of a test command, if the command runs tests.
pub fn detect(command: &str) -> Option<TestCommand> {
    let text = normalize(command);
    let pipefail = pipefail_on(&text);
    let lists = split_lists(&text);
    for (i, (pipeline, before)) in lists.iter().enumerate() {
        let stages: Vec<&str> = pipeline.split('|').collect();
        for (j, stage) in stages.iter().enumerate() {
            let Some(runner) = runner_of(stage) else {
                continue;
            };
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
    static SAFE: OnceLock<Regex> = OnceLock::new();
    let safe = SAFE.get_or_init(|| {
        re(r"^(?:cd|pushd|export|set|source|\.)(?:\s|$)|^(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s*)+$")
    });
    let mut k = i;
    while k > 0 && lists[k].1 == Some("&&") {
        k -= 1;
        let p = lists[k]
            .0
            .trim()
            .trim_start_matches(['(', '{'])
            .trim_start();
        if !p.is_empty() && !safe.is_match(p) {
            return true;
        }
    }
    false
}

/// The command without here-documents, quoted text, comments and redirections
/// that contain `&` or `|`. The text of `bash -c "..."` becomes part of the command.
fn normalize(command: &str) -> String {
    static INLINE: OnceLock<Regex> = OnceLock::new();
    static REDIRECT: OnceLock<Regex> = OnceLock::new();
    let c = crate::checks::strip_heredocs(command);
    let c = INLINE
        .get_or_init(|| {
            re(r#"\b(?:bash|sh|zsh)(?:\s+-[a-z]+)*\s+-[a-z]*c\s+(?:"((?:[^"\\]|\\.)*)"|'([^']*)')"#)
        })
        .replace_all(&c, " ( ${1}${2} ) ");
    let c = blank_quotes(&c);
    let c = REDIRECT
        .get_or_init(|| re(r"\d?>&\d|&>>?"))
        .replace_all(&c, " ");
    c.replace(">|", ">").replace("|&", "|")
}

/// Quoted text becomes `''` or `""`, so that `|` or `;` in a string is not a
/// separator. An apostrophe inside double quotes stays inside them. A comment
/// ends at the end of its line.
fn blank_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut prev = ' ';
    while let Some(c) = chars.next() {
        match c {
            // An escaped separator, for example `\;` in `find -exec`, is not a separator.
            '\\' if chars.peek().is_some_and(|n| "|;&\n'\" ".contains(*n)) => {
                out.push('_');
                chars.next();
            }
            '\'' => {
                for n in chars.by_ref() {
                    if n == '\'' {
                        break;
                    }
                }
                out.push_str("''");
            }
            '"' => {
                let mut escaped = false;
                for n in chars.by_ref() {
                    match n {
                        _ if escaped => escaped = false,
                        '\\' => escaped = true,
                        '"' => break,
                        _ => {}
                    }
                }
                out.push_str("\"\"");
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
                format!(r"^{P}gradlew?(?:\.bat)?\s(?:.*\s)?(?:test|check)"),
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
                self.counts(line, failed, passed);
            }
            Kind::Cargo => {
                let failed = count(line, &["failed"]);
                let passed = count(line, &["passed"]);
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
        let summary = |outcome: Outcome, (line, family): Found| Summary {
            outcome,
            line,
            family,
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
