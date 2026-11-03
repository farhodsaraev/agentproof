//! The full report: one self-contained HTML file with no external requests,
//! so that it can be attached to a pull request or sent by email.
//!
//! All text for users comes from `messages.rs` and follows ASD-STE100.
//! All text from the transcript and the repository is escaped.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

use crate::ai::ClaimVerdict;
use crate::changes::{FileDiff, Origin};
use crate::check::{AiStatus, CheckResult};
use crate::checks::{self, Finding, Severity, Verdict};
use crate::messages::{self, count, report_text as text};
use crate::time::{clock, format_utc};
use crate::transcript::{EventKind, Session};

/// Most items in "What to verify".
const VERIFY_MAX: usize = 5;
/// Lines of code around a finding.
const CONTEXT_LINES: u32 = 3;

pub fn render(r: &CheckResult, diffs: &[FileDiff]) -> String {
    let mut h = String::with_capacity(64 * 1024);
    let project = r
        .project
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| r.project.to_string_lossy().into_owned());
    let title = messages::verdict_title(r.verdict, &r.findings);

    let _ = write!(
        h,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>Agentproof report: {}</title><style>{CSS}</style></head><body><main>",
        esc(&project)
    );
    let _ = write!(
        h,
        "<header><div class=\"brand\">Agentproof report</div><div class=\"meta\">{} · {} · session {}</div></header>",
        esc(&project),
        format_utc(r.checked_at),
        esc(r.session_id.get(..8).unwrap_or(&r.session_id)),
    );

    verdict(&mut h, r, &title);
    summary(&mut h, r);
    verify(&mut h, &r.findings);
    red_flags(&mut h, r, diffs);
    claims(&mut h, r);
    blast_radius(&mut h, r, diffs);
    timeline(&mut h, r.session.as_ref());

    let _ = write!(
        h,
        "<footer><p>{}</p><p>Agentproof {} · transcript: <code>{}</code></p></footer></main></body></html>",
        text::LOCAL_ONLY,
        r.agentproof_version,
        esc(&r
            .transcript_path
            .as_ref()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()),
    );
    h
}

fn verdict(h: &mut String, r: &CheckResult, title: &str) {
    let class = match r.verdict {
        Verdict::DontTrust => "bad",
        Verdict::Check => "warn",
        Verdict::NoRedFlags => "ok",
        Verdict::Safe => "ok",
        Verdict::Incomplete => "none",
    };
    let mut chips = String::new();
    for s in [
        Severity::Critical,
        Severity::High,
        Severity::Medium,
        Severity::Low,
    ] {
        let n = r.findings.iter().filter(|f| f.severity == s).count();
        if n > 0 {
            let _ = write!(
                chips,
                "<span class=\"chip {}\">{n} {}</span>",
                sev_class(s),
                messages::severity_text(s)
            );
        }
    }
    let _ = write!(
        h,
        "<section class=\"verdict {class}\"><h1>{}</h1><p>{}</p><div class=\"chips\">{chips}</div><p class=\"note\">{}</p></section>",
        esc(&capitalize(title)),
        messages::verdict_explanation(r.verdict),
        text::RULES_ONLY,
    );
}

fn summary(h: &mut String, r: &CheckResult) {
    let mut lines: Vec<String> = Vec::new();
    if let Some(s) = &r.session {
        if let Some(first) = s.prompts.first() {
            let asked = short(&first.text, 240);
            lines.push(if s.prompts.len() > 1 {
                format!(
                    "You gave {} prompts. The first prompt was: “{}”",
                    s.prompts.len(),
                    esc(&asked)
                )
            } else {
                format!("Your request was: “{}”", esc(&asked))
            });
        }
    }
    if let Some(c) = &r.changes {
        let files = c.session_files().count();
        let (a, d) = r.lines_added_and_deleted;
        lines.push(if files == 0 {
            text::NO_CHANGES.into()
        } else {
            format!(
                "The agent changed {}: {} added and {} deleted.",
                count(files, "file", "files"),
                count(a as usize, "line", "lines"),
                d
            )
        });
    }
    if let Some(s) = &r.session {
        let mut l = format!(
            "The agent ran {}.",
            count(s.commands.len(), "command", "commands")
        );
        if r.failed_commands > 0 {
            let _ = write!(l, " {} of the commands failed.", r.failed_commands);
        }
        lines.push(l);
        if let Some(secs) = r.session_duration_secs {
            lines.push(format!(
                "The session took {}.",
                count((secs / 60).max(1) as usize, "minute", "minutes")
            ));
        }
    }
    if !r.start_point_found && r.changes.is_some() {
        lines.push(format!(
            "<span class=\"warn-text\">{}</span>",
            text::NO_START
        ));
    }
    let _ = write!(h, "<section><h2>Summary</h2>");
    for l in lines {
        let _ = write!(h, "<p>{l}</p>");
    }
    h.push_str("</section>");
}

fn verify(h: &mut String, findings: &[Finding]) {
    h.push_str("<section><h2>What to verify</h2>");
    let items: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.severity <= Severity::Medium)
        .take(VERIFY_MAX)
        .collect();
    if items.is_empty() {
        let _ = write!(h, "<p>{}</p>", text::NOTHING_TO_VERIFY);
    } else {
        h.push_str("<ol class=\"checklist\">");
        for f in items {
            let _ = write!(
                h,
                "<li><label><input type=\"checkbox\"> <span class=\"chip {}\">{}</span> {} <span class=\"loc\">{}</span></label></li>",
                sev_class(f.severity),
                messages::severity_text(f.severity),
                esc(&f.text),
                esc(&messages::location_text(f).unwrap_or_default()),
            );
        }
        h.push_str("</ol>");
    }
    h.push_str("</section>");
}

fn red_flags(h: &mut String, r: &CheckResult, diffs: &[FileDiff]) {
    let _ = write!(
        h,
        "<section><h2>Red flags <span class=\"count\">{}</span></h2>",
        r.findings.len()
    );
    if r.findings.is_empty() {
        let _ = write!(h, "<p>{}</p>", text::NO_FINDINGS);
    }
    for f in &r.findings {
        let _ = write!(
            h,
            "<article class=\"finding\"><div class=\"head\"><span class=\"chip {}\">{}</span> <strong>{}</strong></div><div class=\"loc\">{}</div>",
            sev_class(f.severity),
            messages::severity_text(f.severity),
            esc(&f.text),
            esc(&messages::location_text(f).unwrap_or_default()),
        );
        if f.other_origin {
            let _ = write!(h, "<p class=\"note\">{}</p>", messages::OTHER_ORIGIN);
        }
        if let Some(code) = file_context(f, diffs) {
            let _ = write!(h, "<pre class=\"diff\">{code}</pre>");
        } else if let Some(cmd) = command_context(f, r.session.as_ref()) {
            let _ = write!(h, "<pre>{cmd}</pre>");
        } else if !f.evidence.is_empty() {
            let _ = write!(h, "<pre>{}</pre>", esc(&f.evidence));
        }
        h.push_str("</article>");
    }
    h.push_str("</section>");
}

/// The changed lines around a file finding, as escaped HTML.
fn file_context(f: &Finding, diffs: &[FileDiff]) -> Option<String> {
    let file = f.file.as_deref()?;
    let line = f.line?;
    let d = diffs.iter().find(|d| d.path == file)?;
    let near = |n: u32| n + CONTEXT_LINES >= line && n <= line + CONTEXT_LINES;
    let mut out = String::new();
    for (n, t) in d.removed.iter().filter(|(n, _)| near(*n)) {
        let _ = writeln!(
            out,
            "<span class=\"del\">-{n:>5}  {}</span>",
            esc(&short(t, 160))
        );
    }
    for (n, t) in d.added.iter().filter(|(n, _)| near(*n)) {
        let _ = writeln!(
            out,
            "<span class=\"add\">+{n:>5}  {}</span>",
            esc(&short(t, 160))
        );
    }
    (!out.is_empty()).then_some(out)
}

/// The command and its error for a transcript finding, as escaped HTML.
fn command_context(f: &Finding, s: Option<&Session>) -> Option<String> {
    let line = f.transcript_line?;
    let c = s?.commands.iter().find(|c| c.line == line)?;
    let mut out = format!("$ {}", esc(&short(&c.command, 600)));
    if let Some(e) = &c.error {
        let _ = write!(out, "\n<span class=\"del\">{}</span>", esc(&short(e, 400)));
    }
    Some(out)
}

fn claims(h: &mut String, r: &CheckResult) {
    h.push_str("<section><h2>Claimed and done</h2><div class=\"cols\">");
    let said = r
        .session
        .as_ref()
        .and_then(|s| s.final_message.as_deref())
        .map(|m| esc(&short(m, 1500)))
        .unwrap_or_default();
    let _ = write!(
        h,
        "<div><h3>What the agent said at the end</h3><blockquote>{said}</blockquote></div>"
    );

    let mut facts = Vec::new();
    if let Some(s) = &r.session {
        let runs: Vec<_> = s
            .commands
            .iter()
            .filter(|c| checks::test_tool(&c.command).is_some())
            .collect();
        match runs.last() {
            None => facts.push("The agent did not run the tests.".to_string()),
            Some(last) => {
                facts.push(format!(
                    "The agent ran the tests {}.",
                    count(runs.len(), "time", "times")
                ));
                facts.push(match last.failed {
                    Some(true) => "The last test run failed.".into(),
                    Some(false) => "The last test run passed.".into(),
                    None => "The result of the last test run is not in the transcript.".into(),
                });
            }
        }
        if let Some(phrase) = s.final_message.as_deref().and_then(checks::pass_claim) {
            facts.push(format!("The final message says “{phrase}”."));
        }
    }
    h.push_str("<div><h3>What the transcript shows</h3><ul>");
    for f in facts {
        let _ = write!(h, "<li>{}</li>", esc(&f));
    }
    h.push_str("</ul></div></div>");
    ai_claims(h, &r.ai);
    h.push_str("</section>");
}

/// The AI opinions, visually apart from the facts above them.
fn ai_claims(h: &mut String, ai: &AiStatus) {
    h.push_str("<div class=\"ai\"><h3>AI check <span class=\"tag\">opinion</span></h3>");
    match ai {
        AiStatus::Off => {
            let _ = write!(h, "<p class=\"note\">{}</p>", text::AI_OFF);
        }
        AiStatus::NotNeeded => {
            let _ = write!(h, "<p class=\"note\">{}</p>", text::AI_NOT_NEEDED);
        }
        AiStatus::Running => {
            let _ = write!(h, "<p class=\"note\">{}</p>", text::AI_RUNNING);
        }
        AiStatus::Done(r) => {
            if let Some(e) = &r.error {
                let _ = write!(
                    h,
                    "<p class=\"warn-text\">The AI check failed. The reason is: {}</p>",
                    esc(e)
                );
            } else if r.claims.is_empty() {
                h.push_str(
                    "<p class=\"note\">The AI check found no claims in the final message.</p>",
                );
            } else {
                h.push_str("<table><thead><tr><th>Claim of the agent</th><th>AI verdict</th><th>Reason and evidence</th></tr></thead><tbody>");
                for c in &r.claims {
                    let (label, class) = match c.verdict {
                        ClaimVerdict::Supported => ("Supported", "ok"),
                        ClaimVerdict::Unsupported => ("Unsupported", "medium"),
                        ClaimVerdict::Contradicted => ("Contradicted", "critical"),
                    };
                    let _ = write!(
                        h,
                        "<tr><td>{}</td><td><span class=\"chip {class}\">{label}</span></td><td>{}<div class=\"loc\">{}</div></td></tr>",
                        esc(&c.claim),
                        esc(&c.reason),
                        esc(&c.evidence)
                    );
                }
                h.push_str("</tbody></table>");
            }
            let mut note = format!("{} Model: {}.", text::AI_LABEL, esc(&r.model));
            if r.removed_secrets > 0 {
                let _ = write!(
                    note,
                    " Agentproof removed {} before the check.",
                    count(r.removed_secrets, "secret", "secrets")
                );
            }
            if r.shortened_input {
                note.push_str(" The input was too long, so Agentproof sent only a part of it.");
            }
            let _ = write!(h, "<p class=\"note\">{note}</p>");
        }
    }
    h.push_str("</div>");
}

fn blast_radius(h: &mut String, r: &CheckResult, diffs: &[FileDiff]) {
    h.push_str("<section><h2>Blast radius</h2>");
    let Some(c) = &r.changes else {
        h.push_str("</section>");
        return;
    };
    let outside: Vec<String> = match (&r.session, &r.start_point) {
        (Some(s), Some(sp)) => checks::outside_request_files(s, diffs, &sp.repo_root)
            .map(|(o, _)| o.iter().map(|d| d.path.clone()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let files: Vec<_> = c.session_files().collect();
    if files.is_empty() {
        let _ = write!(h, "<p>{}</p>", text::NO_CHANGES);
    } else {
        h.push_str("<table><thead><tr><th>File</th><th>Change</th><th>Lines</th><th>Notes</th></tr></thead><tbody>");
        let mut sorted = files.clone();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        for f in sorted {
            let change = match f.status {
                'A' => "added",
                'D' => "deleted",
                _ => "changed",
            };
            let lines = match (f.added, f.deleted) {
                (Some(a), Some(d)) => {
                    format!("<span class=\"plus\">+{a}</span> <span class=\"minus\">−{d}</span>")
                }
                _ => "binary".into(),
            };
            let mut tags = Vec::new();
            if r.session
                .as_ref()
                .is_some_and(|s| !checks::changed_by_agent(s, &f.path))
            {
                tags.push("<span class=\"tag\">not touched by the agent</span>".to_string());
            }
            if checks::is_test_file(&f.path) {
                tags.push("<span class=\"tag\">test</span>".to_string());
            }
            if let Some(area) = checks::sensitive_area(&f.path) {
                tags.push(format!(
                    "<span class=\"tag warn\">{}</span>",
                    messages::area_text(area)
                ));
            }
            if outside.contains(&f.path) {
                tags.push(format!(
                    "<span class=\"tag\">{}</span>",
                    text::NOT_IN_REQUEST
                ));
            }
            if f.origin == Origin::BeforeAndSession {
                tags.push("<span class=\"tag\">also changed before the session</span>".to_string());
            }
            let _ = write!(
                h,
                "<tr><td><code>{}</code></td><td>{change}</td><td class=\"num\">{lines}</td><td>{}</td></tr>",
                esc(&f.path),
                tags.join(" ")
            );
        }
        h.push_str("</tbody></table>");
    }
    let before: Vec<_> = c
        .files
        .iter()
        .filter(|f| f.origin == Origin::BeforeSession)
        .collect();
    if !before.is_empty() {
        let _ = write!(
            h,
            "<details><summary>{} with earlier changes</summary><p>{}</p><ul>",
            count(before.len(), "file", "files"),
            text::BEFORE_SESSION
        );
        for f in before {
            let _ = write!(h, "<li><code>{}</code></li>", esc(&f.path));
        }
        h.push_str("</ul></details>");
    }
    h.push_str("</section>");
}

fn timeline(h: &mut String, s: Option<&Session>) {
    let Some(s) = s else { return };
    let _ = write!(
        h,
        "<section><h2>What happened</h2><details><summary>Show all {} of the session</summary><ol class=\"timeline\">",
        count(s.events.len(), "step", "steps")
    );
    for e in &s.events {
        let (label, class) = match (e.kind, e.failed) {
            (EventKind::Prompt, _) => ("You", "prompt"),
            (EventKind::Message, _) => ("Agent", "message"),
            (EventKind::Command, Some(true)) => ("Command failed", "failed"),
            (EventKind::Command, _) => ("Command", "command"),
            (EventKind::Edit, _) => ("Edit", "edit"),
            (EventKind::Tool, _) => ("Tool", "tool"),
        };
        let time = e.at.map(clock).unwrap_or_default();
        let _ = write!(
            h,
            "<li class=\"{class}\"><span class=\"when\">{time} · line {}</span><span class=\"kind\">{label}</span><span class=\"what\">{}</span></li>",
            e.line,
            esc(&e.text)
        );
    }
    h.push_str("</ol></details></section>");
}

fn sev_class(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "critical",
        Severity::High => "high",
        Severity::Medium => "medium",
        Severity::Low => "low",
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn short(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() > max {
        format!("{}…", t.chars().take(max).collect::<String>())
    } else {
        t.to_string()
    }
}

/// Escape text for HTML. All text from the transcript and the repository goes through this.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Open the report in the default web browser.
pub fn open(path: &Path) -> Result<()> {
    let p = path.as_os_str();
    let status = match std::env::consts::OS {
        // `start` needs Windows path separators.
        "windows" => std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path.to_string_lossy().replace('/', "\\"))
            .status(),
        "macos" => std::process::Command::new("open").arg(p).status(),
        _ => std::process::Command::new("xdg-open").arg(p).status(),
    };
    status.context("cannot open the web browser")?;
    Ok(())
}

const CSS: &str = r#"
:root{--bg:#f7f7f5;--card:#fff;--ink:#1c1c1a;--muted:#6b6b66;--line:#e4e3de;--code:#f1f0ec;
--crit:#b42318;--crit-bg:#fdecea;--high:#c4320a;--high-bg:#fff0e6;--med:#a15c07;--med-bg:#fef6e4;
--low:#475467;--low-bg:#eef1f4;--ok:#1d6b4f;--ok-bg:#e8f5ef;--add:#1d6b4f;--del:#b42318;color-scheme:light}
@media (prefers-color-scheme:dark){:root{--bg:#161615;--card:#1f1f1d;--ink:#ecebe6;--muted:#a3a29b;--line:#34332f;--code:#262624;
--crit:#ff8a80;--crit-bg:#3a1714;--high:#ffab70;--high-bg:#3a2210;--med:#f5c26b;--med-bg:#36290f;
--low:#b3bfcc;--low-bg:#252b31;--ok:#7fd1ae;--ok-bg:#14301f;--add:#7fd1ae;--del:#ff8a80;color-scheme:dark}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.55 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
main{max-width:920px;margin:0 auto;padding:24px 16px 48px}
header{display:flex;flex-wrap:wrap;gap:8px 16px;align-items:baseline;justify-content:space-between;margin-bottom:16px}
.brand{font-weight:700;letter-spacing:.02em}.meta{color:var(--muted);font-size:13px}
section{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:18px 20px;margin:14px 0}
h1{font-size:28px;line-height:1.2;margin:0 0 8px}h2{font-size:17px;margin:0 0 12px}h3{font-size:14px;margin:0 0 8px;color:var(--muted);font-weight:600}
.verdict{border-width:2px}.verdict.bad{border-color:var(--crit);background:var(--crit-bg)}.verdict.bad h1{color:var(--crit)}
.verdict.warn{border-color:var(--med);background:var(--med-bg)}.verdict.warn h1{color:var(--med)}
.verdict.ok{border-color:var(--ok);background:var(--ok-bg)}.verdict.ok h1{color:var(--ok)}
.verdict.none{border-color:var(--low);background:var(--low-bg)}
.chips{display:flex;flex-wrap:wrap;gap:6px;margin:10px 0}
.chip{display:inline-block;font-size:12px;font-weight:600;padding:2px 8px;border-radius:999px;white-space:nowrap}
.chip.critical{color:var(--crit);background:var(--crit-bg);border:1px solid var(--crit)}.chip.high{color:var(--high);background:var(--high-bg)}
.chip.medium{color:var(--med);background:var(--med-bg)}.chip.low{color:var(--low);background:var(--low-bg)}.chip.ok{color:var(--ok);background:var(--ok-bg)}
.ai{border:1px dashed var(--line);border-radius:10px;padding:12px 14px;margin-top:14px}
.note{color:var(--muted);font-size:13px;margin:8px 0 0}.warn-text{color:var(--med)}
.count{color:var(--muted);font-weight:400}
.checklist{padding-left:22px;margin:0}.checklist li{margin:6px 0}.loc{color:var(--muted);font-size:13px}
.finding{border-top:1px solid var(--line);padding:12px 0}.finding:first-of-type{border-top:0;padding-top:0}
.finding .head{display:flex;gap:8px;align-items:baseline;flex-wrap:wrap}
pre{background:var(--code);border-radius:8px;padding:10px 12px;overflow-x:auto;font:12.5px/1.5 ui-monospace,SFMono-Regular,Consolas,monospace;margin:8px 0 0;white-space:pre-wrap;word-break:break-word}
code{font:13px ui-monospace,SFMono-Regular,Consolas,monospace;word-break:break-all}
.add{color:var(--add)}.del{color:var(--del)}.plus{color:var(--add)}.minus{color:var(--del)}
.cols{display:grid;grid-template-columns:1fr 1fr;gap:16px}@media (max-width:640px){.cols{grid-template-columns:1fr}}
blockquote{margin:0;padding:10px 12px;border-left:3px solid var(--line);background:var(--code);border-radius:0 8px 8px 0;white-space:pre-wrap;max-height:320px;overflow:auto}
ul{margin:0;padding-left:20px}
table{width:100%;border-collapse:collapse;font-size:14px}th,td{text-align:left;padding:7px 8px;border-bottom:1px solid var(--line);vertical-align:top}
th{color:var(--muted);font-weight:600;font-size:12px}td.num{white-space:nowrap}
.tag{display:inline-block;font-size:11.5px;padding:1px 7px;border-radius:6px;background:var(--low-bg);color:var(--low);margin:1px 2px}.tag.warn{background:var(--med-bg);color:var(--med)}
details summary{cursor:pointer;color:var(--muted)}
.timeline{list-style:none;padding:0;margin:12px 0 0}.timeline li{display:grid;grid-template-columns:150px 110px 1fr;gap:10px;padding:6px 0;border-top:1px solid var(--line);font-size:13.5px}
.timeline .when{color:var(--muted);font-size:12px}.timeline .kind{font-weight:600}.timeline .what{white-space:pre-wrap;word-break:break-word}
.timeline .failed .kind{color:var(--crit)}.timeline .prompt .kind{color:var(--ok)}
@media (max-width:640px){.timeline li{grid-template-columns:1fr}table{font-size:13px}}
footer{color:var(--muted);font-size:12.5px;padding:8px 4px}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html() {
        assert_eq!(
            esc("<script>alert('x')</script> & \"y\""),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;y&quot;"
        );
    }
}
