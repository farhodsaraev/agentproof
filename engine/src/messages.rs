//! All text that Agentproof shows to users. The text follows ASD-STE100
//! Simplified Technical English: max 20 words in an instruction, max 25 words
//! in a description, one topic per sentence. The tests below check the length.

use crate::checks::{Area, Danger, Finding, Rule, Severity, Unverified, Verdict};
use crate::testruns::{Basis, Outcome, TestRun};

/// The number of findings in the short message. All findings are in the report.
const SHOWN_FINDINGS: usize = 3;

pub struct Summary<'a> {
    pub verdict: Verdict,
    /// All findings, for the counts.
    pub findings: &'a [Finding],
    /// The findings that this message can show, most severe first.
    pub shown: &'a [&'a Finding],
    /// True if `shown` has only the findings that are new since the last message.
    pub only_new: bool,
    pub no_start: bool,
    pub no_git: bool,
    pub no_transcript: bool,
    /// The AI check runs in the background, so the report updates later.
    pub ai_running: bool,
    pub report_path: &'a str,
}

/// The short message in Claude Code.
pub fn summary(s: &Summary) -> String {
    let mut out = vec![verdict_text(s.verdict, s.findings)];
    if s.no_git {
        out.push(
            "This folder is not a Git repository. Thus Agentproof cannot examine the changes."
                .into(),
        );
    }
    if s.no_transcript {
        out.push("Agentproof cannot read the session transcript.".into());
    }

    // Show Low findings only when there is nothing more important.
    let important: Vec<&Finding> = s
        .shown
        .iter()
        .copied()
        .filter(|f| f.severity <= Severity::Medium)
        .collect();
    let list = if important.is_empty() {
        s.shown.to_vec()
    } else {
        important
    };
    for (i, f) in list.iter().take(SHOWN_FINDINGS).enumerate() {
        let new = if s.only_new && i == 0 { "New. " } else { "" };
        out.push(format!(
            "{new}{}: {}",
            severity_text(f.severity),
            with_location(f)
        ));
    }
    if s.findings.len() > SHOWN_FINDINGS.min(list.len()) {
        out.push(format!(
            "The report has all {}.",
            count(s.findings.len(), "finding", "findings")
        ));
    }

    if s.ai_running {
        out.push("The AI check runs now. The report shows its result when it is complete.".into());
    }
    if s.no_start && !s.no_git {
        out.push(
            "There is no start point for this session. Thus Agentproof used the last commit."
                .into(),
        );
    }
    if s.verdict == Verdict::NoRedFlags {
        out.push("This result uses rule checks only.".into());
    }
    out.push(format!("Report: {}", s.report_path));
    out.join(" ")
}

pub fn verdict_text(v: Verdict, findings: &[Finding]) -> String {
    format!("Agentproof: {}.", verdict_title(v, findings))
}

/// The verdict in a few words, for the message and the report.
pub fn verdict_title(v: Verdict, findings: &[Finding]) -> String {
    match v {
        Verdict::DontTrust => "do not trust this work".into(),
        Verdict::Check => {
            let n = findings
                .iter()
                .filter(|f| f.severity <= Severity::Medium)
                .count();
            format!("examine {}", count(n, "item", "items"))
        }
        Verdict::NoRedFlags => "no red flags found".into(),
        Verdict::Safe => "safe to merge".into(),
        Verdict::Incomplete => "incomplete, because some checks did not run".into(),
    }
}

/// One sentence that explains the verdict, for the report.
pub fn verdict_explanation(v: Verdict) -> &'static str {
    match v {
        Verdict::DontTrust => "Agentproof found a critical problem. Do not accept this work before you examine the critical findings.",
        Verdict::Check => "Agentproof found items that need your attention. Examine each item before you accept the work.",
        Verdict::NoRedFlags => "The rule checks found no problems. This does not prove that the work is correct, because the AI check did not confirm the claims of the agent.",
        Verdict::Safe => "The rule checks found no problems, and the AI check found evidence for each claim of the agent. Examine the changes as usual.",
        Verdict::Incomplete => "Agentproof could not read the transcript or the changes. Thus some checks did not run.",
    }
}

pub fn severity_text(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "Critical",
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
    }
}

/// A short location in parentheses, for the message.
/// The finding text with its location in parentheses before the full stop.
fn with_location(f: &Finding) -> String {
    let mut out = match location_text(f) {
        Some(l) => format!("{} ({l}).", f.text.trim_end_matches('.')),
        None => f.text.clone(),
    };
    if f.other_origin {
        out.push(' ');
        out.push_str(OTHER_ORIGIN);
    }
    if f.named_in_request {
        out.push(' ');
        out.push_str(NAMED_IN_REQUEST);
    }
    out
}

/// The note for a sensitive file that the request named.
pub const NAMED_IN_REQUEST: &str = "The request named this file, so this change is expected.";

/// The note for a finding on a file that the agent did not touch.
pub const OTHER_ORIGIN: &str =
    "The agent did not touch this file, so a different session or a person possibly made this change.";

/// The location of a finding, without punctuation.
pub fn location_text(f: &Finding) -> Option<String> {
    let place = match (&f.file, f.line, f.transcript_line) {
        (Some(file), Some(line), _) if line > 0 => Some(format!("{file}, line {line}")),
        (Some(file), _, _) => Some(file.clone()),
        (None, _, Some(t)) => Some(format!("transcript line {t}")),
        _ => None,
    };
    match (place, f.in_subagent) {
        (Some(p), true) => Some(format!("{p}, in a subagent")),
        (None, true) => Some("in a subagent".into()),
        (place, false) => place,
    }
}

/// The text of one finding.
pub fn finding_text(rule: &Rule) -> String {
    match rule {
        Rule::TestFileDeleted => "The agent deleted a test file.".into(),
        Rule::TestsRemoved { count: n } => {
            format!("The agent removed {}.", count(*n, "test", "tests"))
        }
        Rule::AssertionsRemoved { count: n } => {
            format!(
                "The agent removed {}.",
                count(*n, "test assertion", "test assertions")
            )
        }
        Rule::TestSkipped { marker } => {
            format!("The agent added a marker that skips a test ({marker}).")
        }
        Rule::TestFocused { marker } => {
            format!("The agent added a marker that runs only some of the tests ({marker}).")
        }
        Rule::TestsFailedButClaimedPass => {
            "The tests failed, but the agent said that the tests pass.".into()
        }
        Rule::TestsFailedAtEnd => {
            "A test run failed, and no later run of the same tests shows a pass.".into()
        }
        Rule::NarrowPassAfterFailure => {
            "A test run failed. A later run passed, but it did not run all of the same tests."
                .into()
        }
        Rule::PassClaimWithoutTests { phrase } => {
            format!("The agent said \"{phrase}\", but it did not run the tests.")
        }
        Rule::PassClaimUnverified { phrase, reason } => format!(
            "The agent said \"{phrase}\", but no test run passed. {}",
            unverified_text(*reason)
        ),
        Rule::DangerousCommand { kind } => {
            format!("The agent ran a dangerous command: {}.", danger_text(*kind))
        }
        Rule::DependenciesChanged => "The agent changed the dependencies.".into(),
        Rule::SensitiveFile { area } => {
            format!("The agent changed a sensitive file ({}).", area_text(*area))
        }
        Rule::SuppressionsAdded { count: n, marker } => format!(
            "The agent added {} ({marker}).",
            count(*n, "warning suppression", "warning suppressions")
        ),
        Rule::LargeDeletion { lines } => format!("The agent deleted {lines} lines in one file."),
        Rule::OutsideRequest { count: n, folders } => format!(
            "The agent changed {} outside the folders in the request ({}).",
            count(*n, "file", "files"),
            folders
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Rule::RepeatedError { count: n } => format!("The same error occurred {n} times."),
        Rule::AiContradicted { claim, reason } => format!(
            "AI opinion: the evidence does not agree with the claim \"{}\". {reason}",
            short_claim(claim)
        ),
        Rule::AiUnsupported { claim, reason } => format!(
            "AI opinion: there is no evidence for the claim \"{}\". {reason}",
            short_claim(claim)
        ),
    }
}

fn unverified_text(u: Unverified) -> &'static str {
    match u {
        Unverified::NoTests => "The last test run found no tests.",
        Unverified::HiddenExitCode => {
            "The shell hid the exit code, and the output has no test summary."
        }
        Unverified::Background => {
            "The tests ran in the background, and the transcript has no result."
        }
        Unverified::NoResult => "The transcript has no result for the test run.",
    }
}

/// The result of the last test run, for the report. It says where the result comes from.
pub fn last_test_run(t: &TestRun) -> String {
    let line = t.summary.as_deref().map(short_claim);
    let summary = line
        .as_deref()
        .map(|l| format!(" The summary line is “{l}”."))
        .unwrap_or_default();
    match (t.outcome, t.basis, line) {
        (Outcome::Passed, Basis::Summary, Some(l)) => {
            format!("The summary of the last test run shows a pass: “{l}”.")
        }
        (Outcome::Failed, Basis::Summary, Some(l)) => {
            format!("The summary of the last test run shows a failure: “{l}”.")
        }
        (Outcome::Passed, _, _) => "The last test run passed with exit code 0.".into(),
        (Outcome::Failed, _, _) => {
            format!("The last test run failed. The command ended with an error.{summary}")
        }
        (Outcome::NoTests, _, _) => format!("The last test run found no tests.{summary}"),
        (Outcome::Unknown, Basis::HiddenExitCode, _) => format!(
            "The result of the last test run is not known. {}",
            unverified_text(Unverified::HiddenExitCode)
        ),
        (Outcome::Unknown, Basis::Background, _) => format!(
            "The result of the last test run is not known. {}",
            unverified_text(Unverified::Background)
        ),
        (Outcome::Unknown, _, _) => {
            "The result of the last test run is not in the transcript.".into()
        }
    }
}

fn danger_text(d: Danger) -> &'static str {
    match d {
        Danger::RecursiveDelete => "recursive delete",
        Danger::ForcePush => "force push",
        Danger::HardReset => "hard reset",
        Danger::CleanUntracked => "removal of untracked files",
        Danger::PipeToShell => "a downloaded script that runs immediately",
        Danger::Sudo => "a command with administrator rights",
    }
}

pub fn area_text(a: Area) -> &'static str {
    match a {
        Area::Authentication => "authentication",
        Area::Payments => "payments",
        Area::Migrations => "database migration",
        Area::Ci => "CI configuration",
        Area::Secrets => "secrets",
        Area::AgentConfig => "agent configuration",
    }
}

pub fn engine_error(reason: &str) -> String {
    format!("Agentproof cannot examine this session. The reason is: {reason}")
}

pub fn report_opened(path: &str) -> String {
    format!("Agentproof opened the report in your web browser. The report is in {path}.")
}

/// A claim in at most 12 words, so that the sentence stays short.
fn short_claim(claim: &str) -> String {
    let words: Vec<&str> = claim.split_whitespace().collect();
    if words.len() > 12 {
        format!("{}…", words[..12].join(" "))
    } else {
        words.join(" ").trim_end_matches('.').to_string()
    }
}

/// Strict mode: the text that the agent gets when Agentproof stops it.
pub fn block_reason(findings: &[&Finding]) -> String {
    let mut out = vec![
        "Agentproof stopped the end of this turn, because it found a critical problem.".to_string(),
        "Correct the problem, or tell the user about it before you stop.".to_string(),
    ];
    for f in findings {
        out.push(format!(
            "{}: {}",
            severity_text(f.severity),
            with_location(f)
        ));
    }
    out.join(" ")
}

pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Fixed sentences in the report. They are here so that the tests examine them.
pub mod report_text {
    pub const RULES_ONLY: &str = "This report uses rule checks only. Rule checks find facts in the transcript and in the changes, without AI.";
    pub const RULES_AND_AI: &str = "This report uses rule checks and the AI check. The rule checks find facts. The AI check gives opinions.";
    pub const AI_OFF: &str = "The AI check is off. To turn it on, add the AI settings to the user settings file. The README tells you how.";
    pub const AI_RUNNING: &str =
        "The AI check runs now. Open the report again in one or two minutes.";
    pub const AI_NOT_NEEDED: &str =
        "The AI check did not run, because there is no final message or no change.";
    pub const AI_LABEL: &str = "These verdicts are the opinion of an AI model. They are not facts. Examine the evidence before you accept them.";
    pub const NO_FINDINGS: &str = "The rule checks found no problems.";
    pub const NOTHING_TO_VERIFY: &str =
        "Agentproof found no item that needs extra attention. Examine the changes as usual.";
    pub const NO_START: &str = "There is no start point for this session. Thus Agentproof compared the changes with the last commit only.";
    pub const NO_CHANGES: &str = "The agent did not change files in the repository.";
    pub const BEFORE_SESSION: &str = "These files had changes before the session. Agentproof does not count them as work of the agent.";
    pub const NOT_IN_REQUEST: &str = "not in the request";
    pub const LOCAL_ONLY: &str =
        "Agentproof made this report on your computer. It sent no data to a server.";
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const MAX_WORDS: usize = 25;

    pub(crate) fn assert_ste(text: &str) {
        for sentence in text.split(". ").map(|s| s.trim_end_matches('.')) {
            let words = sentence.split_whitespace().count();
            assert!(
                words <= MAX_WORDS,
                "{words} words (max {MAX_WORDS}): {sentence}"
            );
        }
    }

    fn all_rules() -> Vec<Rule> {
        let mut rules = vec![
            Rule::TestFileDeleted,
            Rule::TestsRemoved { count: 2 },
            Rule::AssertionsRemoved { count: 1 },
            Rule::TestSkipped {
                marker: "#[ignore".into(),
            },
            Rule::TestFocused {
                marker: ".only(".into(),
            },
            Rule::TestsFailedButClaimedPass,
            Rule::TestsFailedAtEnd,
            Rule::NarrowPassAfterFailure,
            Rule::PassClaimWithoutTests {
                phrase: "all tests pass".into(),
            },
            Rule::PassClaimUnverified {
                phrase: "all tests pass".into(),
                reason: Unverified::NoTests,
            },
            Rule::PassClaimUnverified {
                phrase: "all tests pass".into(),
                reason: Unverified::HiddenExitCode,
            },
            Rule::PassClaimUnverified {
                phrase: "all tests pass".into(),
                reason: Unverified::Background,
            },
            Rule::PassClaimUnverified {
                phrase: "all tests pass".into(),
                reason: Unverified::NoResult,
            },
            Rule::DependenciesChanged,
            Rule::SuppressionsAdded {
                count: 3,
                marker: "@ts-ignore".into(),
            },
            Rule::LargeDeletion { lines: 400 },
            Rule::OutsideRequest {
                count: 4,
                folders: vec!["src/login".into(), "docs".into()],
            },
            Rule::RepeatedError { count: 5 },
            Rule::AiContradicted {
                claim: "I fixed the login bug and added three tests that cover all the edge cases of the new session code".into(),
                reason: "The last test run failed.".into(),
            },
            Rule::AiUnsupported {
                claim: "No behavior changed".into(),
                reason: "The transcript does not show a test run.".into(),
            },
        ];
        for kind in [
            Danger::RecursiveDelete,
            Danger::ForcePush,
            Danger::HardReset,
            Danger::CleanUntracked,
            Danger::PipeToShell,
            Danger::Sudo,
        ] {
            rules.push(Rule::DangerousCommand { kind });
        }
        for area in [
            Area::Authentication,
            Area::Payments,
            Area::Migrations,
            Area::Ci,
            Area::Secrets,
            Area::AgentConfig,
        ] {
            rules.push(Rule::SensitiveFile { area });
        }
        rules
    }

    fn finding(rule: Rule, severity: Severity) -> Finding {
        Finding {
            text: finding_text(&rule),
            file: Some("src/auth/session_store.ts".into()),
            line: Some(120),
            ..Finding::new(rule, severity)
        }
    }

    #[test]
    fn test_run_texts_follow_ste100_length_limits() {
        let long = "===== 1 failed, 10 passed, 3 warnings, 2 errors, 4 skipped in 12.34s =====";
        for outcome in [
            Outcome::Passed,
            Outcome::Failed,
            Outcome::NoTests,
            Outcome::Unknown,
        ] {
            for basis in [
                Basis::Summary,
                Basis::ExitCode,
                Basis::HiddenExitCode,
                Basis::Background,
                Basis::NoResult,
            ] {
                for summary in [None, Some(long.to_string())] {
                    let t = TestRun {
                        runner: "pytest".into(),
                        exit_code_hidden: false,
                        outcome,
                        basis,
                        summary,
                        filters: Vec::new(),
                        narrowed: false,
                        dir: None,
                    };
                    assert_ste(&last_test_run(&t));
                }
            }
        }
    }

    #[test]
    fn a_piped_run_never_says_that_the_last_test_run_passed() {
        let piped = |outcome, basis, summary: Option<&str>| {
            last_test_run(&TestRun {
                runner: "jest".into(),
                exit_code_hidden: true,
                outcome,
                basis,
                summary: summary.map(Into::into),
                filters: Vec::new(),
                narrowed: false,
                dir: None,
            })
        };
        let unknown = piped(Outcome::Unknown, Basis::HiddenExitCode, None);
        assert!(!unknown.contains("passed"), "{unknown}");
        let pass = piped(
            Outcome::Passed,
            Basis::Summary,
            Some("Tests: 5 passed, 5 total"),
        );
        assert!(
            pass.starts_with("The summary of the last test run shows a pass"),
            "{pass}"
        );
        let fail = piped(
            Outcome::Failed,
            Basis::Summary,
            Some("Tests: 2 failed, 2 total"),
        );
        assert!(fail.contains("shows a failure"), "{fail}");
    }

    #[test]
    fn a_subagent_location_is_named() {
        let mut f = Finding::new(Rule::TestsFailedAtEnd, Severity::High);
        f.in_subagent = true;
        assert_eq!(location_text(&f).as_deref(), Some("in a subagent"));
        f.transcript_line = Some(4);
        assert_eq!(
            location_text(&f).as_deref(),
            Some("transcript line 4, in a subagent")
        );
    }

    fn sample<'a>(
        findings: &'a [Finding],
        shown: &'a [&'a Finding],
        verdict: Verdict,
    ) -> Summary<'a> {
        Summary {
            verdict,
            findings,
            shown,
            only_new: false,
            no_start: true,
            no_git: false,
            no_transcript: false,
            ai_running: false,
            report_path: "C:/Users/x/.agentproof/p/s/report.html",
        }
    }

    #[test]
    fn every_finding_text_follows_ste100_length_limits() {
        for rule in all_rules() {
            let f = finding(rule, Severity::High);
            let line = format!("{}: {}", severity_text(f.severity), with_location(&f));
            assert!(!line.contains(". ("), "{line}");
            assert_ste(&line);
        }
    }

    #[test]
    fn every_fixed_text_follows_ste100_length_limits() {
        use report_text::*;
        for t in [
            RULES_ONLY,
            RULES_AND_AI,
            AI_OFF,
            AI_RUNNING,
            AI_NOT_NEEDED,
            AI_LABEL,
            NO_FINDINGS,
            NOTHING_TO_VERIFY,
            NO_START,
            NO_CHANGES,
            BEFORE_SESSION,
            LOCAL_ONLY,
        ] {
            assert_ste(t);
        }
        for v in [
            Verdict::DontTrust,
            Verdict::Check,
            Verdict::NoRedFlags,
            Verdict::Safe,
            Verdict::Incomplete,
        ] {
            assert_ste(verdict_explanation(v));
        }
        assert_ste(&engine_error("git diff failed"));
        assert_ste(&report_opened("C:/x/report.html"));
    }

    #[test]
    fn every_summary_follows_ste100_length_limits() {
        let findings: Vec<Finding> = all_rules()
            .into_iter()
            .map(|r| finding(r, Severity::Medium))
            .collect();
        let shown: Vec<&Finding> = findings.iter().collect();
        for v in [
            Verdict::DontTrust,
            Verdict::Check,
            Verdict::NoRedFlags,
            Verdict::Safe,
            Verdict::Incomplete,
        ] {
            assert_ste(&summary(&sample(&findings, &shown, v)));
            assert_ste(&summary(&Summary {
                no_git: true,
                no_transcript: true,
                ..sample(&[], &[], v)
            }));
        }
    }

    #[test]
    fn summary_shows_the_top_three_and_points_to_the_report() {
        let findings: Vec<Finding> = (0..5)
            .map(|_| finding(Rule::DependenciesChanged, Severity::Medium))
            .collect();
        let shown: Vec<&Finding> = findings.iter().collect();
        let text = summary(&sample(&findings, &shown, Verdict::Check));
        assert!(text.starts_with("Agentproof: examine 5 items."), "{text}");
        assert_eq!(
            text.matches("Medium: The agent changed the dependencies (")
                .count(),
            3
        );
        assert!(text.contains("The report has all 5 findings."), "{text}");
        assert!(
            text.ends_with("Report: C:/Users/x/.agentproof/p/s/report.html"),
            "{text}"
        );
    }

    #[test]
    fn new_findings_are_marked_and_old_ones_are_not_repeated() {
        let findings = [
            finding(
                Rule::DangerousCommand {
                    kind: Danger::ForcePush,
                },
                Severity::Critical,
            ),
            finding(Rule::DependenciesChanged, Severity::Medium),
        ];
        let shown = [&findings[1]];
        let text = summary(&Summary {
            only_new: true,
            no_start: false,
            ..sample(&findings, &shown, Verdict::DontTrust)
        });
        assert!(
            text.contains("New. Medium: The agent changed the dependencies (src/auth/session_store.ts, line 120)."),
            "{text}"
        );
        assert!(!text.contains("force push"), "{text}");
        assert!(text.contains("The report has all 2 findings."), "{text}");
    }

    #[test]
    fn low_findings_show_only_when_nothing_is_more_important() {
        let findings = [
            finding(Rule::DependenciesChanged, Severity::Medium),
            finding(Rule::RepeatedError { count: 3 }, Severity::Low),
        ];
        let shown: Vec<&Finding> = findings.iter().collect();
        let text = summary(&sample(&findings, &shown, Verdict::Check));
        assert!(!text.contains("Low:"), "{text}");
    }

    #[test]
    fn no_red_flags_is_never_called_safe() {
        let text = summary(&sample(&[], &[], Verdict::NoRedFlags));
        assert!(text.contains("no red flags found"));
        assert!(text.contains("rule checks only"));
        assert!(!text.to_lowercase().contains("safe"));
    }
}
