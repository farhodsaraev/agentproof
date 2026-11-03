//! All text that Agentproof shows to users. The text follows ASD-STE100
//! Simplified Technical English: max 20 words in an instruction, max 25 words
//! in a description, one topic per sentence. The tests below check the length.

use crate::checks::{Area, Danger, Finding, Rule, Severity, Verdict};

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
        Verdict::Incomplete => "incomplete, because some checks did not run".into(),
    }
}

/// One sentence that explains the verdict, for the report.
pub fn verdict_explanation(v: Verdict) -> &'static str {
    match v {
        Verdict::DontTrust => "Agentproof found a critical problem. Do not accept this work before you examine the critical findings.",
        Verdict::Check => "Agentproof found items that need your attention. Examine each item before you accept the work.",
        Verdict::NoRedFlags => "The rule checks found no problems. This does not prove that the work is correct, because there is no AI check yet.",
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
    match location_text(f) {
        Some(l) => format!("{} ({l}).", f.text.trim_end_matches('.')),
        None => f.text.clone(),
    }
}

/// The location of a finding, without punctuation.
pub fn location_text(f: &Finding) -> Option<String> {
    match (&f.file, f.line, f.transcript_line) {
        (Some(file), Some(line), _) if line > 0 => Some(format!("{file}, line {line}")),
        (Some(file), _, _) => Some(file.clone()),
        (None, _, Some(t)) => Some(format!("transcript line {t}")),
        _ => None,
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
        Rule::TestsFailedAtEnd => "The last run of the tests failed.".into(),
        Rule::PassClaimWithoutTests { phrase } => {
            format!("The agent said \"{phrase}\", but it did not run the tests.")
        }
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

pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Fixed sentences in the report. They are here so that the tests examine them.
pub mod report_text {
    pub const RULES_ONLY: &str = "This report uses rule checks only. Rule checks find facts in the transcript and in the changes, without AI.";
    pub const NO_AI: &str = "This section needs the AI check. The AI check comes in a later version. Until then, compare the claim with the evidence yourself.";
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
            Rule::PassClaimWithoutTests {
                phrase: "all tests pass".into(),
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
            rule,
            severity,
            file: Some("src/auth/session_store.ts".into()),
            line: Some(120),
            transcript_line: None,
            evidence: String::new(),
        }
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
            NO_AI,
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
