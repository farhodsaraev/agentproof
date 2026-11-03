# Agentproof: Project Description

Nov 3, 2026 · @Farhod

Name: Agentproof (chosen Nov 3, 2026; free on crates.io). Domains, plugin marketplaces and trademarks are not checked yet.

Status: milestone 4 is done (Nov 3, 2026), except a test with a real API key. The macOS build from milestone 1 is not done.

## Overview

Agentproof is an open-source plugin for Claude Code, used inside the Claude desktop app, that tells a developer whether to trust what their AI coding agent just did. When the agent finishes, Agentproof automatically compares what the agent *claimed* against what it *actually did*, flags risky changes, and gives a verdict: safe, check these, or don't trust.

**One-line pitch:** Your agent says "done." Agentproof shows the proof.

|  |  |
| --- | --- |
| Type | Claude Code plugin (works in the Claude desktop app's Code tab and in the terminal), open source (MIT or Apache-2.0) |
| Engine | A small Rust program bundled with the plugin, which does all the checking |
| First target | Claude Code; later Codex, OpenCode, Gemini CLI (the engine also works as a standalone command for those) |
| Users | Solo developers and small teams who let agents write most of their code |
| Runs | Locally; the only network call is to the LLM provider the user chooses, and it works without one |
| Output | A short verdict inside the app, a full self-contained HTML report, and JSON |

What makes it different from every existing tool: it reads the agent's own session transcript, not just the git diff. The diff shows what changed; the transcript shows what the agent was asked, what it tried, what failed, and what it told you. Nobody else uses that second source.

Why a plugin first: the check happens at the exact moment the developer decides whether to accept the work, with no command to remember. Claude Code hands the plugin the session transcript directly, so there is no guessing which session to read.

## The problem

Agents now write most of the code in many projects, but checking that code is slower than generating it, so people stop checking. 96% of developers say they don't fully trust AI-generated code, yet only 48% always check it before committing. *(Source to confirm and cite before publishing: believed to be Sonar's 2026 State of Code developer survey.)*

The moment of failure is specific: the agent touches 40 files, says "Done, all tests pass," and the developer has to decide in seconds whether to accept it. Today they skim the diff, run the tests again by hand, or just trust it.

The diff alone cannot answer the questions that matter:

- Did it actually run the tests, and what did the output say?
- Why did a "fix the login bug" task touch the billing module?
- Did it hit an error, fail to fix it, and quietly delete the failing test instead?
- Did it run anything dangerous (`rm -rf`, `git push --force`, `curl | sh`)?
- Does its summary match the code it wrote?

Anthropic's own system cards document models special-casing or modifying tests to make them pass. This is not a hypothetical failure; it is a known behaviour that gets rarer as models improve but does not disappear. An independent check from outside the agent is the only reliable answer.

## How it works

The plugin hooks into two moments of a Claude Code session and also adds a command to run it by hand.

```
Session starts ──► Agentproof notes where the repo stands (commit + changed files)
       │
Agent works, may commit along the way
       │
Agent finishes a turn ──► Agentproof runs, if code changed since the last check
       │
       ├─ reads the session transcript   (what was asked, done, said)
       ├─ reads the changes since start  (what actually changed)
       ├─ runs rule checks               (facts, no AI)
       ├─ runs one AI check, optional    (claims vs. evidence)
       └─ shows the verdict in the app + writes the full report
```

1. **Remember the starting point.** When a session starts, a session-start hook records the current commit and which files were already changed. This matters because agents often commit their own work: comparing against the latest commit would then show nothing and the report would look clean when it isn't.
2. **Run when the agent stops.** Claude Code's stop hook fires every time the agent finishes a turn and passes the plugin the session id, the transcript location and the project folder. Agentproof runs only if code changed since its last check, so chatty sessions don't produce a report per message. It must finish in a few seconds without the AI check.
3. **Collect the changes** from the recorded starting point to the current state, including uncommitted work. Changes the developer had already made before the session are marked as such, not blamed on the agent.
4. **Run rule checks** on the transcript and the changes. These need no model and always give the same answer for the same input.
5. **Run one AI check** (optional) that reads the user's request, the agent's final summary, the commands it ran with their output, and the changes, then reports mismatches. Secrets are stripped before anything is sent.
6. **Show the result.** A one-line verdict with the top findings appears in the app. The full report is saved as one HTML file (outside the repo) and can be opened from there. JSON is written alongside it for scripts.

Manual use: a `/agentproof:check` command runs the same check on demand, for example against another base (`/agentproof:check main`, later) or an earlier session.

Optional strict mode: on a "Don't trust this" verdict, the plugin can stop the agent from ending its turn and hand it the findings to fix. Agentproof still does the judging; the agent only gets the list. Off by default.

## The report

The report is the product. It must be readable in 30 seconds and good enough to screenshot. Sections, top to bottom:

| Section | What it shows | Source |
| --- | --- | --- |
| Verdict | One of: **Safe to merge**, **Check these N spots**, **Don't trust this**. Colour-coded, at the top. Without the AI check, the best possible verdict is **No red flags found**, never "Safe" | All checks combined |
| Summary | 3–4 plain sentences: what you asked, what the agent did, how long it took, how many files it touched | Transcript + changes |
| Claimed vs. done | A two-column list. Left: statements the agent made ("added tests", "all passing", "no behaviour change"). Right: what the evidence shows, with a tick, a warning or a cross | AI over transcript + changes |
| Red flags | Rule-check findings, each with severity, file and line, and the transcript moment it came from | Rule checks |
| What happened | A collapsed timeline of the session: prompts, tool calls, commands and their exit codes, errors and retries. Expand any step | Transcript |
| Blast radius | Files changed, grouped by the intent they served, and files touched that the request never mentioned | Changes + transcript |
| What to verify | A checklist of the 2–5 places worth a human's eyes, ordered by risk | All checks |

Design rules: evidence links back to the exact transcript line or diff hunk; AI judgements are visually distinct from rule-check facts; no finding without a location; the whole thing is one HTML file with no external requests, so it can be attached to a PR or emailed.

Writing rules: all report text is written in ASD-STE100 Simplified Technical English. This covers the verdict, the summary, every finding, the AI judgements and the in-app message.

- Sentences have a maximum of 20 words for instructions ("What to verify") and 25 words for descriptions.
- Active voice, simple tenses, one instruction per sentence.
- The same word for the same thing every time, and one meaning per word. A fixed glossary in the engine defines the terms (for example "change", "finding", "test command").
- Rule-check messages are fixed templates, written and reviewed once against STE100.
- The AI check is told to answer in STE100. The engine then checks sentence length and rejects or shortens text that breaks the limit.
- The same rules apply to all other text the user sees: error messages, warnings, the `/agentproof:check` command help, the README and the documentation (principle 8).
- All fixed user text is kept in one place in the engine. An automatic test checks every message for sentence length and glossary terms, so a pull request that breaks the rules fails.
- The contributor guide explains the STE100 rules, so outside contributors write new messages the same way.

In the app, only the short version shows by default: verdict, the number of findings by severity, and the top three. Showing the full report in the app's side panel is the goal; how far a plugin can do that is to be confirmed in M1.

## Checks

Two layers with different trust levels, always kept separate in the report.

### Rule checks (fact, no model)

| Check | Severity | Source |
| --- | --- | --- |
| Tests deleted, skipped, focused (`.only`), or assertions removed | High | Changes |
| Test command ran and exited non-zero, and no later run of it passed | Critical | Transcript |
| Agent's final message uses a known "tests pass" phrase but no test command appears in the transcript | High | Transcript |
| Dangerous commands: `rm -rf`, `git push --force`, `git reset --hard`, `curl \| sh`, `sudo` | Critical | Transcript |
| New or changed dependencies (Cargo.toml, package.json, requirements.txt, go.mod, etc.) | Medium | Changes |
| Edits to sensitive paths: auth, payments, migrations, CI config, `.env`, secrets, agent config (`CLAUDE.md`, `.claude/`) | Medium | Changes |
| Lint, type or coverage suppressions added (`as any`, `# type: ignore`, `#[allow]`, `eslint-disable`) | Medium | Changes |
| Large deletions (configurable threshold) | Medium | Changes |
| Files changed outside the directories the request named | Low | Changes + transcript |
| Same error hit 3+ times with retries (sign of thrashing) | Low | Transcript |

The "tests pass" check uses a fixed, configurable list of phrases, so it stays a rule. Claims worded any other way are left to the AI check.

### AI judgements (opinion, labelled as such)

One structured call to a model the user chooses. Input: the user's prompts, the agent's final summary, every command with its output (truncated), and the changes. Output: a list of claims with a verdict each (supported, unsupported, contradicted) and a one-line reason pointing at evidence.

Rules for this call:

- **It is never the agent checking itself.** The plugin makes this call directly with the user's own API key. It never asks the Claude session that wrote the code to review its work.
- **Secrets are removed before sending.** Anything that looks like a key, token, password or private key, plus the contents of `.env` and similar files, is replaced with a placeholder. The report lists what was removed.
- **No key, no AI check.** The layer is skipped and the report says so. The verdict then rests on rule checks alone and is never better than "No red flags found".

## Architecture

Two parts: a thin plugin that plugs into Claude Code, and an engine that does all the work. Keeping them apart means the engine can be tested on its own and reused for other agents.

### The plugin

| Piece | What it does |
| --- | --- |
| Plugin manifest | Name, version, description; installable from a plugin marketplace |
| Session-start hook | Calls the engine to record the starting point |
| Stop hook | Calls the engine to check the session; shows the short verdict; optionally blocks in strict mode |
| `/agentproof:check` command | Runs a check by hand, with an optional base or session |
| Launcher | Picks the right engine build for Windows, macOS or Linux |

### The engine (Rust)

Organised as a pipeline of small modules. Each module has its own tests and sample data so it can be built and checked one at a time.

| Module | Responsibility | Key crates |
| --- | --- | --- |
| `cli` | Entry points the hooks call (`start`, `check`, `report`), plus standalone use | `clap` |
| `transcript` | Parse Claude Code session files: prompts, assistant messages, tool calls, tool results, exit codes. Isolated so format changes break only here | `serde_json`; possibly the `claude-transcript` crate |
| `diff` | Collect changes since the recorded start and classify files (test, source, config, dependency, sensitive) | shell out to `git`, or `git2` |
| `checks` | Rule checks over transcript + changes, each returning findings with severity, location and evidence | plain Rust |
| `redact` | Strip secrets from anything sent to the AI check | `regex` |
| `llm` | One structured call to a configurable provider (Anthropic, OpenAI, OpenAI-compatible, Ollama). Graceful skip without a key | `reqwest`, `serde` |
| `report` | Render HTML, write JSON, produce the short in-app summary | `askama` or `minijinja` |
| `config` | `.agentproof.toml` for sensitive paths, thresholds, phrases, provider, model, strict mode | `serde`, `toml` |

Data flow: hooks → `cli` → `transcript` + `diff` → `checks` (+ `redact` → `llm`) → `report`. No module below `checks` knows about HTML; no module above `report` knows about the model.

Where things are stored: the starting point, reports and JSON live in Agentproof' own folder in the user's home directory, keyed by project and session. Nothing is written inside the repo.

Adapters for other agents are a later concern: each agent gets its own `transcript::<agent>` parser producing the same internal session type, so checks and the report never change.

Estimated size: v1 around 2,000–3,000 lines of Rust including tests, plus a small amount of plugin configuration.

## Scope

### v1 (ship in 5 weeks)

- Claude Code plugin, working in the desktop app and the terminal
- Automatic check when the agent finishes a turn with code changes; `/agentproof:check` to run it by hand
- Changes measured from the session's start, including the agent's own commits
- All rule checks listed above
- One AI check for claimed-vs-done, with secret removal, skippable
- Short verdict in the app, full HTML report, JSON
- `.agentproof.toml` config
- Windows, macOS and Linux from day one
- Optional strict mode (off by default)

### Explicitly not in v1

- Other agents
- Rerunning tests ourselves
- Session history, trends, dashboards
- CI mode or PR comments
- Any server, account or telemetry

### Later versions

| Version | Adds |
| --- | --- |
| v1.1 | Full report inside the app's side panel, if not possible in v1; standalone command documented for non-plugin use |
| v1.2 | Codex and OpenCode adapters |
| v1.3 | Rerun tests in a clean worktree; fail-before/pass-after proof for new tests |
| v2 | Session history and trust trends per project; GitHub Action posting the report on PRs |
| v2+ | Open proof format shared with other tools; local web UI |

## Principles

1. **The checker is never the agent.** Agentproof never asks the agent that wrote the code whether the code is good. Rule checks need no model; the AI check is a separate call with the user's key.
2. **Fact before opinion.** Rule-check findings always rank above AI judgements, and the report shows which is which.
3. **Every finding has evidence.** A file and line, or a transcript line. No location, no finding.
4. **Local by default.** The only outbound call is to the model the user chose, with secrets removed first. No telemetry, no account, no server.
5. **Never touch the repo.** Agentproof reads; it never stages, commits, stashes or edits, and stores its own files outside the repo.
6. **Absence of a verdict is not a pass.** If a step fails or is skipped, the report says so instead of showing green.
7. **Never slow the agent down.** Without the AI check, a run finishes in a few seconds. The AI check never blocks unless strict mode is on.
8. **Every word to the user is ASD-STE100.** All text that Agentproof gives to a user follows ASD-STE100 Simplified Technical English. This includes the report, the in-app message, error messages, warnings, command help, the README and the documentation. No exceptions.
9. **Small and finished beats big and half-done.** Each release does one more thing fully.

## Development environment

Development happens in a Docker container on the local computer. The container holds the tools. The project files stay on the host.

| Item | Details |
| --- | --- |
| Container | Rust 1.99 on Debian, with rustfmt, clippy and the tools to build the Windows version |
| Start | `docker compose run --rm dev` opens a shell in the container. VS Code can also open the folder as a dev container |
| Project files | The project folder on the host is connected to `/workspace` in the container. Edit on the host, build in the container |
| Build output | Kept in a Docker volume, not in the project folder, because builds in a Windows folder are slow |
| Linux and Windows builds | The container builds both. Tested on Nov 3, 2026: a Windows program built in the container runs on the Windows host |
| macOS build | Not possible in the container. A hosted build service (GitHub Actions) makes it later |
| Files | `compose.yaml`, `.devcontainer/Dockerfile`, `.devcontainer/devcontainer.json` |

Limit: the plugin itself is not tested in the container. The Claude desktop app runs on the host and writes the session files there. The final plugin test therefore uses the real app on the host, with the engine that the container built.

All contributors use the same container, so all contributors get the same results.

## Roadmap

Draft, assuming one developer with Claude Code, starting the week of Nov 9, 2026.

| Milestone | Week | Done when |
| --- | --- | --- |
| M1: Skeleton | 1 | Plugin installs on Windows, macOS and Linux; session-start and stop hooks call the engine; the engine reads a real transcript and the changes since start; we know how the result can be shown in the app |
| M2: Rule checks | 2 | Every rule check works against sample sessions, including made-up "bad agent" sessions that should be caught |
| M3: Report | 3 | Short in-app verdict, full HTML report and JSON; looks good enough to screenshot |
| M4: AI check | 4 | Claimed-vs-done works with at least Anthropic and one OpenAI-compatible provider; secret removal tested; strict mode works |
| Dogfood | 5 | Used on our own work every day, including building Agentproof itself; fix what hurts; first real catches saved as screenshots |
| Launch | end of 5 | Public repo, README with screenshots, marketplace listing, launch post (Hacker News, X, Reddit) |

Each milestone ends with `cargo test` green, the README updated, and a short public post. Building in public from M1 so launch day has an audience.

## Risks

| Risk | What we do |
| --- | --- |
| Claude Code changes its transcript format | All parsing lives in one module with sample files from several versions; break loudly, never silently |
| Stop hook fires on every turn | Run only when code changed since the last check; keep the rule-check run fast |
| Shipping a Rust program inside a plugin across three systems | Solve in M1, before any checks are written |
| Secrets leak to the AI provider | Redaction module with its own tests; list everything removed in the report |
| False alarms make people ignore it | Conservative rules, every finding shows evidence, thresholds configurable |

## Success metrics and open questions

### What success looks like

| Horizon | Target |
| --- | --- |
| Launch week | Front page of Hacker News or 500+ GitHub stars; 5 real catches posted as screenshots |
| 3 months | 1,000+ stars; 10+ outside contributors; second agent supported |
| 6 months | Used weekly by people we don't know; featured by at least two newsletters or YouTube channels; Agentproof verifies its own pull requests |

### Open questions

- [ ] Check domains, plugin marketplaces and trademarks for the name Agentproof
- [ ] MIT or Apache-2.0
- [ ] Build on the `claude-transcript` crate or write our own parser
- [ ] How the engine reaches users: built copies stored in the plugin, or downloaded on first run
- [ ] How much of the report a plugin can show inside the desktop app
- [ ] Default AI provider when the user has several keys
- [ ] Which marketplace to list in first

### Decided

- [x] Name: Agentproof (Nov 3, 2026)
- [x] Development happens in a local Docker container (Nov 3, 2026)
- [x] Delivered as a Claude Code plugin first, not a standalone command (Nov 3, 2026)
- [x] Without the AI check the best verdict is "No red flags found", never "Safe"
- [x] Windows supported in v1
- [x] All text that Agentproof gives to users follows ASD-STE100 Simplified Technical English: report, in-app message, errors, help, README and documentation (Nov 3, 2026)

## Milestone 1 progress

Updated Nov 3, 2026.

| Item | Status |
| --- | --- |
| Engine records the start point of a session | Done. Tested |
| Engine reads a real Claude Code transcript | Done. Tested on a real 68-minute session: it found all 14 prompts and 21 commands |
| Engine finds all changes since the start, including commits by the agent | Done. Tested |
| Engine keeps changes from before the session apart | Done. Tested |
| No message when nothing changed since the last check | Done. Tested |
| Plugin files: manifest, hooks, `/agentproof:check` command, launcher, local marketplace | Done |
| Hook sequence on Windows through Git Bash, with simulated hook data | Done. Tested |
| Build for Linux and Windows in the container | Done |
| Install and test in the real Claude desktop app | Done. Tested on Nov 3, 2026: the start hook, the stop hook and `/agentproof:check` operate in the app |
| How the result shows in the app (`systemMessage`) | Done. The app shows it as a "Claude Code notice" with the prefix "Stop says:" |
| macOS build | Not started. Needs a hosted build service |

Facts found during milestone 1:

- Hook commands on Windows run in Git Bash if it is installed, else in PowerShell. The launcher is a shell script, so it needs Git Bash. A PowerShell launcher is an open item.
- The session-start hook output goes into the context of the agent. Thus the start command prints nothing.
- The session-start hook also runs when a session resumes. Thus the engine keeps the first start point and does not replace it.
- A failed command in the transcript has `is_error: true`, and its result starts with `Exit code N`.
- Most transcript lines (372 of 386 in one session) are of types that this version does not use. The parser ignores them and counts them.
- A check on Windows takes approximately 2.5 seconds. Most of this time is for the `git` commands.
- Plugin commands get the plugin name as a prefix. Thus the command is `/agentproof:check`.
- The desktop app has no button to add a local plugin folder, and its plugin screens do not list a local plugin. The working method is two entries in `~/.claude/settings.json`: `extraKnownMarketplaces` (source type `directory`) and `enabledPlugins` (`"agentproof@agentproof": true`).
- The in-app message is a "Claude Code notice" under the last reply of the agent, with the prefix "Stop says:". The message of milestone 1 is long (approximately 60 words). The report of milestone 3 must make it shorter: the verdict and the top findings only.
- In the app test, the engine correctly kept 24 files with earlier changes apart from the 1 file that the agent made.

## Milestone 2 progress

Updated Nov 3, 2026.

| Item | Status |
| --- | --- |
| All ten rule checks from the "Checks" section | Done. Each check has its own tests |
| Verdict: "do not trust this work", "examine N items", "no red flags found", "incomplete" | Done. Without the AI check, the best verdict is "no red flags found", never "safe" |
| Short message with the verdict and the top three findings | Done. Low findings show only when there is nothing more important |
| Full findings in the JSON file, each with a location and evidence | Done |
| "Bad agent" session: failed tests with a claim that they pass, a force push, a skipped test, a new dependency, sensitive files, a suppression, a repeated error, a file outside the request | Done. All are caught, and the verdict is "do not trust this work" |
| "Good agent" session: a failed test that the agent fixed, then a pass | Done. No findings |
| Test on this real session | Done. 1.5 seconds. Three true findings (recursive deletes of temporary folders) and no false alarms |
| `.agentproof.toml` settings for thresholds | Not started. The thresholds are fixed values in the code |

Facts found during milestone 2:

- On Windows, a check of a path that starts with `//` opens a network connection, and it can take 40 seconds. A web link in a prompt caused this. The engine now examines only relative paths in prompts.
- Marker text inside a string, for example a list of markers in a checker, is not a real marker. The engine now ignores markers inside double-quoted strings, and it requires attributes such as `#[ignore]` at the start of the line.
- The first run of a new engine build on Windows took 13 seconds. The next runs took 1.5 seconds. A security scan of the new file is the probable cause.
- Agentproof reported the recursive deletes of this development session correctly. Deletes of temporary folders are still reported. A list of safe folders (for example `node_modules` and `target`) prevents some alarms. More safe targets are a possible later change.
- When the agent changed a file that already had changes before the session, the line checks also see the earlier changes in that file. The start point stores only a content id, not the old text.

## Milestone 3 progress

Updated Nov 3, 2026.

| Item | Status |
| --- | --- |
| Full HTML report: one file, no external requests, light and dark mode | Done. Sections: verdict, summary, what to verify, red flags, claimed and done, blast radius, what happened |
| Evidence in the report: changed lines around each file finding, and the command with its error for each transcript finding | Done |
| All text from the transcript and the repository is escaped in the report | Done. Tested with HTML in a prompt and in a message |
| Short message shows only new findings | Done. A finding that a message showed before does not come again. A manual check shows all findings |
| No message when there are no new findings and the verdict did not change | Done |
| Shorter message: verdict, up to three findings, report path | Done |
| `/agentproof:report` command that opens the report in the web browser | Done. Not tested in the app yet |
| "Claimed and done" with the AI check | Not started. Milestone 4. The section now shows the final message and the test facts from the transcript |
| Blast radius grouped by intent | Not started. It needs the AI check. The table now marks test files, sensitive files and files outside the request |

## Milestone 4 progress

Updated Nov 3, 2026.

| Item | Status |
| --- | --- |
| AI check with Anthropic (default model `claude-opus-5-5`, structured JSON output, server-side fallback on a refusal) | Done. Tested with a fake server |
| AI check with an OpenAI-compatible server (OpenAI, Ollama) | Done. Tested with a fake server |
| Secret removal before the AI check: known key formats, secret values, private keys, passwords in URLs, whole `.env` and key files | Done. Tested |
| The AI check runs in the background, so the agent does not wait | Done. Tested on Windows: the hook returned in 1.2 seconds |
| AI findings in the message, the verdict and the report, marked as opinion | Done |
| Verdict "safe to merge", only when the AI check supports every claim and there are no other problems | Done |
| Strict mode stops the agent once for each new critical finding | Done. Tested |
| Settings: user file `~/.agentproof/config.toml`; project file `.agentproof.toml` (strict mode only) | Done |
| Test with a real API key | Not done. Waiting for an API key from the owner |
| Limits (large deletion, repeated errors) in the settings file | Not started. The limits are fixed in the code |
| AI check with the user's own Claude plan (no API key): optional plugin `agentproof-ai` with a Claude Code agent hook | Done. JSON validated. Not tested in the app yet |

Decisions made during milestone 4:

- Only the user settings file can turn on the AI check. A repository cannot choose the provider, the address or the key, because a cloned repository could otherwise send your code and your key to its own server.
- The API key is only in an environment variable, never in a file.
- The AI check is off by default. An API key in the environment does not turn it on.
- Rust has no official Claude SDK, so the engine uses plain HTTP requests, as the Claude API documentation shows.

AI check with the user's plan, decided Nov 3, 2026:

- The optional plugin `agentproof-ai` uses an agent hook, a documented Claude Code feature. Claude Code starts a separate checker agent with the user's own login. There is no API key and no extra money. It uses plan limits.
- The checker gives a result only to the agent and the chat, not to the report. A contradicted claim makes the agent continue with the reason. The `stop_hook_active` field stops a second check in the same turn, so there is no loop.
- Not chosen for now: the engine starts `claude -p` with the user's subscription. The Claude Code legal page does not permit third-party developers to route requests through Free, Pro, Max or Ultra plan credentials for their users. It is not clear if a plugin that starts `claude -p` is "ordinary use". Ask Anthropic in writing before this option is added. If Anthropic agrees, it replaces the agent hook, so that each turn has only one AI call.

Facts found in the app test of Nov 3, 2026:

- The rule check found the planted false claim: "The agent said "tests pass", but it did not run the tests."
- False alarm, fixed: a document that names `eslint-disable` as text counted as a warning suppression. Documents (`.md`, `.txt` and similar) are now not examined for suppressions.
- False alarm, fixed: two sessions in the same folder. A change from one session counted as work of the agent in the other session. Now a finding on a file that the agent did not touch gets the severity Low and a note: "a different session or a person possibly made this change". The agent touched a file if it edited the file with a file tool, named the file in a command, or ran a command that can change any file (for example a package manager or a formatter). Edits and commands of subagents count too: their transcripts are in a separate folder.
- Missed claim, fixed: the fixed phrase list did not find "I also ran all tests and they pass". Now a sentence is a "tests pass" claim if it has a word that starts with "test", a word for success ("pass", "green", "succeed"), and no negative or uncertain word.
- Plugins load only when a session starts. A new plugin is not active in a session that was already open.
