//! All text that Agentproof shows to users. The text follows ASD-STE100
//! Simplified Technical English: max 20 words in an instruction, max 25 words
//! in a description, one topic per sentence. The tests below check the length.

use crate::checks::{Area, Danger, Finding, Rule, Severity, Verdict};

/// The number of findings in the short message. All findings are in the JSON file.
const SHOWN_FINDINGS: usize = 3;

pub struct Summary<'a> {
    pub verdict: Verdict,
    pub findings: &'a [Finding],
    pub files: usize,
    pub commands: usize,
    pub no_start: bool,
    pub no_git: bool,
    pub no_transcript: bool,
    pub data_path: &'a str,
}

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
    if !s.no_git && !s.no_transcript {
        out.push(format!(
            "The agent changed {} and ran {}.",
            count(s.files, "file", "files"),
            count(s.commands, "command", "commands"),
        ));
    }

    // Show Low findings only when there is nothing more important.
    let important: Vec<&Finding> = s
        .findings
        .iter()
        .filter(|f| f.severity <= Severity::Medium)
        .collect();
    let shown: Vec<&Finding> = if important.is_empty() {
        s.findings.iter().take(SHOWN_FINDINGS).collect()
    } else {
        important.into_iter().take(SHOWN_FINDINGS).collect()
    };
    for f in &shown {
        out.push(format!(
            "{}: {} {}",
            severity_text(f.severity),
            f.text,
            location(f)
        ));
    }
    let more = s.findings.len() - shown.len();
    if more > 0 {
        out.push(format!(
            "The full data has {more} more {}.",
            if more == 1 { "finding" } else { "findings" }
        ));
    }

    if s.no_start && !s.no_git {
        out.push("Agentproof did not find the start point. Thus it compared the changes with the last commit only.".into());
    }
    if s.verdict == Verdict::NoRedFlags {
        out.push(
            "This verdict uses rule checks only. It does not prove that the work is correct."
                .into(),
        );
    }
    out.push(format!("The full data is in {}.", s.data_path));
    out.join(" ")
}

fn verdict_text(v: Verdict, findings: &[Finding]) -> String {
    match v {
        Verdict::DontTrust => "Agentproof verdict: do not trust this work.".into(),
        Verdict::Check => {
            let n = findings
                .iter()
                .filter(|f| f.severity <= Severity::Medium)
                .count();
            format!("Agentproof verdict: examine {}.", count(n, "item", "items"))
        }
        Verdict::NoRedFlags => "Agentproof verdict: no red flags found.".into(),
        Verdict::Incomplete => "Agentproof verdict: incomplete. Some checks did not run.".into(),
    }
}

fn severity_text(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "Critical",
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
    }
}

fn location(f: &Finding) -> String {
    match (&f.file, f.line, f.transcript_line) {
        (Some(file), Some(line), _) if line > 0 => format!("Location: {file}, line {line}."),
        (Some(file), _, _) => format!("Location: {file}."),
        (None, _, Some(t)) => format!("Location: transcript line {t}."),
        _ => String::new(),
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

fn area_text(a: Area) -> &'static str {
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

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_WORDS: usize = 25;

    fn assert_ste(text: &str) {
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

    fn sample<'a>(findings: &'a [Finding], verdict: Verdict) -> Summary<'a> {
        Summary {
            verdict,
            findings,
            files: 1,
            commands: 3,
            no_start: true,
            no_git: false,
            no_transcript: false,
            data_path: "C:/Users/x/.agentproof/p/s/check.json",
        }
    }

    #[test]
    fn every_finding_text_follows_ste100_length_limits() {
        for rule in all_rules() {
            let f = finding(rule, Severity::High);
            assert_ste(&format!(
                "{}: {} {}",
                severity_text(f.severity),
                f.text,
                location(&f)
            ));
        }
    }

    #[test]
    fn every_summary_follows_ste100_length_limits() {
        let findings: Vec<Finding> = all_rules()
            .into_iter()
            .map(|r| finding(r, Severity::Medium))
            .collect();
        for v in [
            Verdict::DontTrust,
            Verdict::Check,
            Verdict::NoRedFlags,
            Verdict::Incomplete,
        ] {
            assert_ste(&summary(&sample(&findings, v)));
            assert_ste(&summary(&Summary {
                no_git: true,
                no_transcript: true,
                ..sample(&[], v)
            }));
        }
        assert_ste(&engine_error("git diff failed"));
    }

    #[test]
    fn summary_shows_the_top_three_and_counts_the_rest() {
        let findings: Vec<Finding> = (0..5)
            .map(|_| finding(Rule::DependenciesChanged, Severity::Medium))
            .collect();
        let text = summary(&sample(&findings, Verdict::Check));
        assert!(
            text.starts_with("Agentproof verdict: examine 5 items."),
            "{text}"
        );
        assert_eq!(
            text.matches("Medium: The agent changed the dependencies.")
                .count(),
            3
        );
        assert!(
            text.contains("The full data has 2 more findings."),
            "{text}"
        );
    }

    #[test]
    fn low_findings_show_only_when_nothing_is_more_important() {
        let findings = [
            finding(Rule::DependenciesChanged, Severity::Medium),
            finding(Rule::RepeatedError { count: 3 }, Severity::Low),
        ];
        let text = summary(&sample(&findings, Verdict::Check));
        assert!(!text.contains("Low:"), "{text}");
        assert!(text.contains("1 more finding."), "{text}");
    }

    #[test]
    fn no_red_flags_is_never_called_safe() {
        let text = summary(&sample(&[], Verdict::NoRedFlags));
        assert!(text.contains("no red flags found"));
        assert!(text.contains("does not prove that the work is correct"));
        assert!(!text.to_lowercase().contains("safe"));
    }
}
