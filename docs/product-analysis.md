# Agentproof: product strategy

Date: 4 November 2026. Reader: the founder. Basis: the fact-checked market research, the product audit, the judged ideas from this workflow and new measurements on the founder's computer (4 November 2026).

After the workflow, I checked the key claims again. Section 13 lists the checks. One claim about the competitor WST2002/proof-of-done was not complete. This file has the correction. The appendix ([analysis/appendix.md](analysis/appendix.md)) has the full audit and all judged ideas.

## 1. Summary

- **Today.** The idea is right, and the hardest part works: the start point keeps the agent's work apart from earlier changes. But Agentproof is not ready. Its main check gives false passes, and nobody can install it.
- **What Agentproof must become.** It must become the local, independent fact check for AI coding agents. After each turn, it answers one question: "Is what the agent said true?" It runs before AI reviewers such as CodeRabbit and does not compete with them.
- **Priority 1: make the core check true.** Read the test output, not only the exit code. Make the verdict honest. Do not give "safe to merge" from an AI opinion. Give "incomplete" when the engine cannot read the transcript.
- **Priority 2: catch test tampering.** Catch the agent that changes a test instead of the code, with the old and new values as evidence. Later, prove it: run the original test against the new code.
- **Priority 3: make the install work.** Put the engine for Windows, macOS and Linux in the plugin release. List Agentproof in the Claude plugin directory.
- **Priority 4: publish accuracy.** Give users one command that checks their past sessions. Publish the precision of each rule, with the sample size.
- **Priority 5: stay independent of one vendor.** Give a stable JSON output and exit codes for CI. Add Codex as the second agent.
- **Plan.** Phase A (weeks 1 to 4) fixes only the P0 blockers, with regression tests. Phase B (weeks 5 to 8) makes the install work, measures precision and ends with a soft launch. The public launch comes in month 4.
- **The most important next step.** This week, stop the false pass. In the founder's own sessions, 495 of 525 test commands (94%) piped their output, so the exit code was not the test result. When a test command uses a pipe, read its output. Do not report "passed" from the exit code. Until you fix this, nobody can trust a "no red flags found" verdict.

## 2. Where Agentproof stands

The market has five groups:

- **Diff reviewers** read the code change and give AI opinions. Examples: CodeRabbit, Greptile and Copilot code review.
- **Agent vendors** add review and safety features inside their own agent: Anthropic, OpenAI, Cursor and GitHub.
- **Session platforms** store or grade agent sessions: Entire, agentsview and gh-aw.
- **Claim checkers** compare what the agent says with the commands it ran: Imbue Vet, Tally and many small Stop hooks.
- **Self-check skills** tell the agent to check its own work before it says "done". Example: Superpowers verification-before-completion.

Agentproof is a claim checker that also reads git from the start of the session.

### Market map

| Tool | What it reads | Rules or AI | Where it runs | Price | What it misses |
|---|---|---|---|---|---|
| **CodeRabbit** (PR app, CLI, IDE, Claude Code plugin) | Diff, repository, rule files | AI | Cloud. The CLI also uses the cloud backend | $24–$72 per user per month (annual). Free for public repositories. The IDE extension works on the Free plan. CLI add-on $0.25 per file | Transcript, test results, claims, commands that leave no diff. One CLI review takes 7–30+ minutes |
| **Other diff reviewers**: Greptile, Graphite, Qodo, Sourcery, Bito, cubic, Gemini Code Assist | Diff and repository. Greptile can read Jira and Linear tickets. Qodo can read business requirements | AI | Cloud; some self-host | Greptile $1/$3/$10 per review. Sourcery $12–$24 per developer per month, free for public repositories. Others vary | Transcript and the user's prompts |
| **GitHub Copilot code review** | Diff and repository. Skips dependency files | AI | github.com and IDEs. A VS Code review of uncommitted changes usually takes less than 30 s | About $0.25–$10 per review in AI credits (estimate). Each run uses AI credits | Transcript. It reads review rules from the PR branch, so a PR can change its own rules |
| **Cursor Bugbot** | Diff, rules, PR comments | AI | Cloud; local /review-bugbot | Usage-based; about $1–$1.50 per review (unconfirmed) | Transcript |
| **Claude Code Review, /code-review, ultrareview** (Anthropic) | Diff and code | AI with a verification step | Anthropic cloud or a local subagent. A managed review takes about 20 minutes | Anthropic bills by tokens. About $15–$25 per managed review (press, unconfirmed). Ultrareview $5–$25 | It does not build or run code. It does not compare claims with results |
| **codex-plugin-cc** (OpenAI; 34.8k stars) | "Claude's response" and changes. OpenAI does not document the exact input | AI gate in a Stop hook | Local, inside Claude Code | ChatGPT plan or OpenAI API key | No evidence trail and no report. Its README warns of long loops that use up plan limits |
| **Superpowers verification-before-completion** (obra/superpowers; 311.8k stars; about 1.0M directory installs) | The agent's own context | A prompt skill. The agent checks its own work | Inside the agent | Free (MIT) | The checker is the agent. No outside evidence, no git rules and no report |
| **Imbue Vet** (515 stars) | Diff plus the conversation from Claude Code, Codex or OpenCode | AI | Local CLI, agent skill, GitHub Action | Free (AGPL-3.0); user's own API key | No fact rules. Not per turn. No test re-run |
| **Entire CLI and Trails** ($60M seed; CLI 5.3k stars) | Transcripts, tool calls and commits from 8+ agents | AI review agents | Local capture; review in the Entire cloud | CLI free (MIT). Entire does not disclose the Trails price; $100 starter credit | No confirmed fact-based claim check. Review needs the cloud |
| **agentsview** (6.3k stars) | Session history of 50+ agents | Rules: a health score out of 100 from penalties, shown as grades A to F. Sessions with no clear result stay unscored (from its docs on 4 Nov 2026) | Local | Free (MIT) | It grades the process, not the claims. No git rules |
| **gh-aw trace graders** (GitHub; 5.6k stars) | Traces of headless runs in GitHub Actions | Rules (experimental) | GitHub Actions | Free (MIT); AI credits for runs | Only headless CI runs. It grades the run, not the claims |
| **Tally** (kru3ish; 0 stars; preview) | Transcript, the summary of the test runner, the diff since the session-start HEAD, its own test run | Rules plus LLM judges | Local plugin and CLI | Free (MIT) | No rules described for dangerous commands, sensitive files or test tampering |
| **Jasowills/receipts** (1 star; no release) | Transcript and git state | Rules; optional LLM | Local CLI; GitHub Action | Free (no license file) | No per-turn hook found. No users yet |
| **WST2002/proof-of-done** (3 stars; MIT; started 16 Oct 2026). The closest rival on test claims | Transcript, test summaries (node --test, Jest, Vitest, pytest, Cargo, Mocha), the git diff from the session-start commit, coverage reports | Rules. A Stop gate: it blocks until the checks pass after the last edit. It finds hidden exit codes (`\| tail`, `\|\| true`), zero-test runs, stale runs, flaky runs and narrow runs in monorepos. It also finds test tampering: skipped or focused tests, weakened or always-passing assertions, deleted tests, hidden errors and lowered coverage thresholds | Local Stop hook (Claude Code plugin) | Free (MIT) | Only tests, build and lint. No rules for dangerous commands, lost data, sensitive files or scope. No report. It does not catch an exact expected value that the agent changed to another exact value, for example the value from the failure |
| **veltiq/groundtruth** (3 stars) | Transcript plus the git diff | Rules | Local Stop hook, CLI, Action and pre-commit hook | Free (MIT) | Few claim types. No tampering chain. No full report |
| **Small claim gates, transcript only**: dheerajjha/show-your-work (4 stars), B0yko/proof-of-done (0 stars), vnmoorthy/groundtruth (7 stars), ablanchard-dev/claimcheck (0 stars) | Transcript | Rules. vnmoorthy/groundtruth is a regex gate for completion claims | Local Stop hooks | Free (MIT or Apache-2.0). ablanchard-dev/claimcheck is GPL-3.0 | No git rules. Only ablanchard-dev/claimcheck publishes accuracy: 0 false alarms on 8,707 turns |
| **Claude Code native safety**: auto mode, protected paths, checkpoints, /goal | Each action; the conversation | AI classifier; prompt hook | Local | Included | It misses 17% (Anthropic) to 81% (one stress test) of risky actions. It does not check edits inside the project. No audit after the turn |
| **Command guards**: dcg (6.2k stars), CC Safety Net (1.9k stars) | Each command before it runs | Rules | Local hook | Free (dcg has a custom license) | Prevention only. No claims and no report |
| **Agentproof today** | Transcript plus git since the session start | Rules; optional AI | Local Stop hook | Free | Test output, binaries, other agents, CI, published accuracy |

Notes:
- Star counts are from 4 November 2026. They change fast.
- "(unconfirmed)" marks facts that come only from third-party sources.
- Three different projects use the name "claimcheck", and two use the name "groundtruth". This report names the owner each time.
- The brief named Tally, dod-guard and show-me-the-work. Tally is kru3ish/tally (confirmed). dod-guard is a workflow kit with no claim checker. The research found no project named show-me-the-work. The nearest is show-your-work.

### What CodeRabbit-like tools do

- They read the diff, the repository and rule files such as CLAUDE.md, AGENTS.md or .coderabbit.yaml.
- They find bugs, security problems and style problems, and they suggest fixes.
- They post summaries and comments on pull requests.
- They now also run before a pull request exists. Examples: CodeRabbit CLI, Copilot in VS Code, Cursor /review-bugbot, Claude Code /code-review and Codex /review.
- CodeRabbit CLI has a fixed output format for agents: NDJSON events, a severity gate and exit code 2.

### What they do not do

- They do not read the agent's transcript.
- They do not know which tests ran, what the output said, or if an edit came after the last run.
- They do not see actions that leave no diff: a force push, a deletion outside the repository, or `curl | sh`.
- They do not see the user's prompts. Some read a PR description or a linked ticket, but not the requests in the session.
- They do not keep the agent's work apart from changes that the user made before.
- Most cost money for each review or each seat. Cloud reviews take minutes: the CodeRabbit CLI takes 7–30+ minutes, and Claude Code Review takes about 20 minutes.
- Even the fast local reviews (Copilot in VS Code, under 30 s) use AI credits on each run. So a review after each turn is expensive.
- Anthropic's managed review states that it does not build or run your code.

### What Agentproof can do that they cannot

- Compare the claim "tests pass" with the real test runs, their output and their order.
- Find test tampering as a sequence: a failure, then a test edit, then a pass.
- Find destructive commands and lost data that leave no diff.
- Check the changed files against the user's own prompts.
- Run after each turn in about 1.5 s, with no tokens and no network. This number is Agentproof's own measurement.

Some claim checkers do parts of this list. Diff reviewers cannot do the items that need the transcript: test runs, the order of events and commands that leave no diff. Some can check scope against a PR description or a ticket, but not against the user's prompts. The two layers fit together. Agentproof checks facts after each turn. An AI reviewer checks code quality before the merge.

## 3. Positioning

**Positioning sentence.** Agentproof is the local fact check for AI coding agents. After each turn, it compares what the agent said with what it did.

**Lead message.** "Your agent says the tests pass. Agentproof shows if that is true."

**Target user.** A developer who lets Claude Code write most of the code. This developer accepts agent work many times each day and cannot read every diff. A second target comes later: small teams that merge pull requests from agents.

**Job to be done.** "When my agent says 'done, all tests pass', tell me in seconds if that is true. Show me what I must check myself."

**Why now:**
- Agents work longer with no person watching. From Claude Code 2.1.283, auto mode is the default start mode for interactive sessions.
- Built-in gates miss risky actions. Anthropic reports that auto mode missed 17% of real overeager actions. One independent stress test measured 81% misses on ambiguous tasks.
- Agents claim passes that no run supports. In one team's 150 sessions, 80 of 357 "tests pass" claims had no run behind them. The team could not check 92 more (#104504; one team, not a population estimate).
- Models change tests to make them pass. On ImpossibleBench, more than 79% of Claude cheats were test edits.
- Developers distrust AI code but often do not check it. 96% do not fully trust it, and only 48% always check it before a commit (Sonar, 2026).
- AI review costs money for each review and takes minutes. A local check after each turn costs nothing.
- Competitors can copy this soon. The simple claim check is now common, and Anthropic already has all the parts to build one.

**What Agentproof is not:**
- It is not an AI pull-request reviewer. It does not look for bugs or style problems in the code.
- It does not prompt before each command. Claude Code auto mode, dcg and CC Safety Net work before commands run.
- Auto mode misses many risky actions, so Agentproof gives the audit after the turn. Later, one opt-in gate asks only before push, PR creation or merge while a proven Critical finding is open.
- It is not a dashboard for cost or usage.
- It is not an agent that fixes code or writes to the repository.
- It is not a cloud service, and it needs no account.

**Against the self-check skill.** Most users already have Superpowers verification-before-completion (311.8k stars; about 1.0M directory installs). The skill asks the agent to check itself. Agentproof checks from outside, with evidence. This is the principle "the checker is never the agent". Agentproof must show what the skill misses, for example with the history audit on sessions where the skill was on.

**Where Agentproof is behind:**
- It has no users. Vet, Entire, agentsview, codex-plugin-cc and Superpowers have hundreds to hundreds of thousands of stars.
- Nobody can install it today. Small rivals install with one command.
- show-your-work and both proof-of-done projects already find piped, stale and zero-test runs. Agentproof does not.
- WST2002/proof-of-done also blocks the common forms of test tampering: skipped tests, weakened assertions and deleted tests. Agentproof does not.
- Tally and Jasowills/receipts already run the tests again. Agentproof does not.
- Vet, Jasowills/receipts and veltiq/groundtruth have GitHub Actions. Agentproof cannot run in CI.
- Vet reads 3 agents, and Entire reads 8 or more. Agentproof reads only Claude Code.
- ablanchard-dev/claimcheck publishes 0 false alarms on 8,707 turns. Agentproof publishes no accuracy numbers.

**Stop claiming these:**
- "Nobody else reads the transcript." At least ten tools do.
- "Transcript plus git is unique." Tally, Jasowills/receipts, veltiq/groundtruth and WST2002/proof-of-done also read git.
- "Safe to merge." Today this verdict can come after test failures that the engine did not see.
- "The report and the JSON file remove secrets." The audit found live keys in both files.
- "The next message shows new AI findings." This is rarely true today.
- "Agentproof catches dangerous commands" as the lead message. Native features and guards stop most of them earlier.
- The dangerous-command rule matters most in bypass mode, headless `-p` and SDK runs, CI and non-Claude agents. Mention it there, not in the main pitch.

**First area to win.** This area is narrower than it looks. WST2002/proof-of-done and Tally already join test output, the order of runs and a git baseline. proof-of-done also blocks skipped tests, weakened assertions and deleted tests. So Agentproof must first match these checks. Then it can win in two places:
- **The copied-value chain.** A test fails, the agent changes the expected value to the value from the failure, and the test passes. The evidence is the old and the new value. No tool in the research catches this case, because the assertion stays exact. Later, a new test run can prove the result.
- **The whole session in one audit.** proof-of-done checks only tests, build and lint. Agentproof also checks data that git cannot restore, sensitive files, scope against the user's prompts and secrets. It gives one verdict and one report.

**Protection against copies.** No single rule protects Agentproof, because a small rival can copy one rule in a few days. The protection is the sum: measured accuracy, depth in one report, an install that works, and independence from one vendor.

**Success in 12 months:**
- 1,400 installs ran a check in the last 30 days. This is the north-star metric (section 9 has the full table).
- Each rule that can give Critical has 90% or more precision on at least 30 labelled positives.
- Users report fewer than 2 false alarms per 100 sessions.
- Agentproof runs after each turn in Claude Code and Codex, and as a CI step.
- It proves test claims with a proof run on Linux, macOS and WSL2.
- It sends nothing to any server unless the user turns on the AI check.

## 4. What blocks production readiness today

The audit ran 10 experiments (e1 to e9, plus e3b) against the built engine, and it read the code. The table lists the blockers in priority order:

- **P0** items give a false pass or leak data. Fix them before anyone else uses Agentproof (Phase A).
- **P1** items stop the public launch. Phase B fixes the P1 items that the soft launch needs. The Next phase fixes the others before the public launch.
- **P2** items cause noise or gaps that users see in the first week.

| # | Level | Blocker | Evidence | Fix |
|---|---|---|---|---|
| 1 | P0 | The main rule trusts the exit code. When a test run pipes its output to tail, the run exits 0, so the failures do not show. The engine also misses common test commands | e1: the output said "Tests: 2 failed", and the agent said "All tests pass". The verdict was "no red flags found". Code: the test-tool list in checks.rs does not match `npm run test:unit`, `bun run test`, `python manage.py test` or `nx test`. A "tests pass" claim after them gives a false High finding. #90659: a background command can report exit 0 after a failure | Keep the output of test commands. Mark a piped run "exit code hidden". Parse the summary line of the test runner. Add the missing test-command forms. Treat a run with zero tests as "not run" |
| 2 | P0 | A narrower passing run clears an earlier full failure | e2: a full run failed. Then a filtered run with `--passWithNoTests` passed. Verdict: "no red flags found" | Compare the scope of the runs. Only the same run or a wider run can clear a failure |
| 3 | P0 | "Safe to merge" can come after test failures that the engine did not see. The AI request labels each exit-0 run "passed" and sends no output. Nobody checks the AI evidence | Code review of the AI request and the verdict logic (lead checks 1 and 5) | Hide "safe to merge" until an executed proof exists. Send the end of the output. Count an AI note only when it cites a real line |
| 4 | P0 | An unknown or empty transcript gives "no red flags found" | e3 and e3b | Add a health gate that gives "incomplete" with the reason |
| 5 | P0 | Live secrets appear in the JSON file and the report. The engine does not remove Stripe-style keys | e6: an Anthropic key 2 times and a Stripe key 4 times | Remove secrets from each saved field. Add key patterns. Fix the order of checks in the secret-file test |
| 6 | P0 | Edits through MCP tools or folder commands get Low as "other origin" | e4: a deleted test folder, a CI change and an auth change gave "no red flags found" | Never lower Critical, High, CI or secret findings to Low. A command on a folder covers each file in the folder |
| 7 | P0 | The engine treats deletion of old, ignored data as "cleanup" | e9: `rm -rf data` deleted a gitignored database. No finding | At the start, record the paths that git ignores and the untracked paths, within a time limit. Remove the exemption for data from before the session |
| 8 | P0 | Test runs in subagents have no results | e7: a subagent's test run failed. Result: the wrong rule and a weaker verdict | Keep the results of subagent commands, and use them in all rules |
| 9 | P1 | Nobody can install Agentproof. The engine is not in the repository, the build needs Docker, there is no macOS build, and Windows needs Git Bash | The .gitignore file, the launcher, the README | Put binaries for 3 systems in the plugin release. Add a PowerShell launcher |
| 10 | P1 | No LICENSE file, no CI and no SECURITY.md | The list of tracked files | Add LICENSE-MIT, LICENSE-APACHE and SECURITY.md. Run CI on 3 systems |
| 11 | P1 | The AI result rarely reaches the user. In strict mode, the AI call can stop the agent for up to 5 minutes | The AI cache key, the 300 s timeout, a weak assertion in the AI e2e test | Show the latest finished AI result. Never wait for the AI check inside a hook |
| 12 | P1 | Nobody has tested the agentproof-ai plugin. It runs on each turn, and its result does not reach the report | README "Limits"; milestone 4 notes | Remove it from the default install until it passes tests |
| 13 | P1 | Nobody has tested the AI check with a real API key | Project status | Test it with Anthropic and with one OpenAI-compatible server. Record the real token counts |
| 14 | P1 | Work that the user asked for gives false alarms. The project's own risk table names alert fatigue as the main risk | e5: the request named the "auth module" and the "CI workflow". The engine still gave 2 Medium findings for these files | Map request words to areas. Lower a matching finding to Low, with the note "You asked for this" and the prompt line |
| 15 | P1 | Data on disk has no limit. check.json keeps a copy of the parsed session in ~/.agentproof: prompts, commands and the final message. n01 adds command output. There is no cleanup command | The audit's data-hygiene note. On 4 Nov 2026, ~/.agentproof held 9.8 MB in 13 session folders. The largest check.json had 1.3 MB | Add `agentproof clean --older-than 30d` and a default limit of 30 days. Add a setting that leaves the full session out of the JSON file. Add a "Data on disk" paragraph to the README |
| 16 | P2 | The engine does not detect many destructive commands, mainly Windows forms. A force push that the user asked for still gives "do not trust this work" | e8: `find -delete`, `git checkout -- .`, `DROP TABLE` and others. The research lists four Windows recursive deletes that reached a drive root or the home folder. #88462: the agent wrote a script with `rm -rf` and then ran it | Add the patterns in the list below, each with a fixture. Scan scripts that the agent wrote and then ran. Read the request before you rate a force push |

**Destructive patterns to add (blocker 16).** Each pattern needs one fixture.
- Windows: `rmdir /s /q`, `del /s`, and `Remove-Item -Recurse -Force` on a drive root or the home folder.
- Paths that resolve to a root: an MSYS path of `\` or `/`, `cygpath` output, 8.3 short names, and empty variables such as `rm -rf "$DIR/"`.
- Git: `git checkout -- .`, `git restore`, `git stash` when the user has changes, and `git branch -D`.
- Files: `find -delete`.
- Data and infrastructure: `DROP` or `TRUNCATE`, `prisma migrate reset`, `terraform destroy`, `aws s3 rm --recursive` and `kubectl delete`.
- Scripts: a script that the agent wrote in the session and then ran (#88462). Scan its text with the same patterns.

Two README claims are false today. The first is that the report removes secrets (blocker 5). The second is that the next message shows new AI findings (blocker 11). Fix both, or remove them before the repository becomes public.

Some known gaps are not blockers. Agentproof has no session history, and users cannot change thresholds. Old sessions with no start point give more false alarms. These gaps can wait.

## 5. Ranked opportunities

The judges scored each idea for value (weight 0.4), differentiation (0.35) and feasibility (0.25), on a scale of 1 to 10. Three judges scored each idea in table A. In table B, one judge per category screened all ideas, so those scores are less reliable. The column "My priority" gives my order. Items 1 and 2 are repairs, not new features.

### Table A: deep-judged ideas

| ID | Idea | One line | Value | Differentiation | Feasibility | Total | Effort | My priority |
|---|---|---|---|---|---|---|---|---|
| n02 | Test-tampering chain | Link "test failed, test edited, test passed", with copied values as proof | 6 | 7 | 6 | 6.4 | M | 3 |
| n01 | Test forensics | Classify each test run from its output: passed, failed, hidden, zero tests, narrowed, stale | 8 | 3 | 7 | 6.0 | M | 1 |
| n04 | Proof levels and verification-debt list | A verdict that names its evidence, plus a list for each file of what nobody checked | 6 | 5 | 7 | 5.9 | S | 2 (verdict part now, levels later) |
| n07 | Safety net | List what git cannot restore, find lost data, ask before deletion, restore | 7 | 4 | 6 | 5.7 | M | 4 (lost-data finding only) |
| n06 | "You said don't" | Check limits from prompts and rule files, and show the rule next to the act | 6 | 4 | 7 | 5.6 | M | Later (11) |
| n05 | Claim ledger | Check each sentence of "done", and find stubs that the agent did not mention | 7 | 4 | 5 | 5.5 | M | 8 (narrow first version) |
| n03 | Proof run | Run the tests again in clean copies: fail before, pass after | 6 | 6 | 3 | 5.3 | L | 10 (on request first) |
| e02 | agentproof wrap | Run any headless agent, then give a verdict and an exit code | 5 | 4 | 7 | 5.2 | S | 7 (a reader, not a wrapper) |
| e05 | ACP tap | A proxy that checks Agent Client Protocol agents in many editors | 5 | 5 | 5 | 5.0 | M | Later, maybe |
| e03 | Guard the guard | Flag changes to agent hooks, permissions, MCP servers and rule files | 5 | 3 | 7 | 4.8 | S | Replaced by n14 |
| e01 | Codex and Cursor adapters | Run the same rules on Codex and Cursor sessions | 6 | 3 | 5 | 4.7 | L | 9 (Codex only) |
| e06 | Editor receipt lens | Show findings and untested lines inside the editor diff | 5 | 4 | 5 | 4.6 | M | Later |
| n11 | History audit | Scan past sessions, and print one honest card of facts | 6 | 2 | 6 | 4.6 | M | 5 |
| e07 | Cross-vendor second opinion | Send the AI check to another vendor, and drop opinions with no citation | 4 | 3 | 7 | 4.4 | S | Citation check only (in item 2) |
| e04 | MCP server | Agents read their own receipt before they say "done" | 4 | 3 | 6 | 4.2 | M | No |
| e09 | Agent scorecard | Compare agents and models on your own code | 4 | 3 | 5 | 3.9 | M | No |
| e11 | Agent Honesty Index | Public counts of false claims for each agent | 3 | 6 | 2 | 3.8 | L | No |
| e10 | Agent duel | Two agents, one task, one evidence report | 4 | 3 | 3 | 3.4 | L | No |
| e08 | Chain of custody | Match each changed hunk to the session that wrote it | 3 | 3 | 4 | 3.2 | L | No |
| e12 | Open Agent Receipt | An open receipt format, a golden test set, receipts on PRs | 3 | 3 | 3 | 3.0 | L | No (publish a JSON schema instead) |

### Table B: screened-only ideas (less reliable scores)

| ID | Idea | One line | Value | Differentiation | Feasibility | Total | Effort | My priority |
|---|---|---|---|---|---|---|---|---|
| n08 | Push gate | Ask the user before a push, PR creation or merge while a proven Critical finding is open | 6 | 4 | 8 | 5.8 | S | Next, if time permits |
| n15 | Secret flow and tripwire key | Trace each secret. A planted fake key proves a leak | 5 | 6 | 5 | 5.4 | M | Secret removal now; diff secret scan next; flow later; no tripwire |
| n16 | Dependency evidence | Name each new package, where its name came from, and if it exists | 5 | 6 | 5 | 5.4 | M | Next, if time permits |
| n18 | Catch Bench | A public labelled set of sessions that scores claim checkers | 6 | 6 | 3 | 5.3 | L | 6 (own precision first) |
| n24 | Evidence card and repro bundle | Share a redacted card of claim and evidence, or a bug bundle | 6 | 4 | 6 | 5.3 | S | Later |
| n13 | Who caused it | Origin and injection chains for risky actions | 5 | 6 | 4 | 5.1 | M | Later (research) |
| n14 | Oversight drift, narrow | Flag changes that turn off checks when the user did not ask for them | 4 | 5 | 7 | 5.1 | S | Later |
| n10 | Trust status line | The trust state in the Claude Code status line | 6 | 3 | 6 | 5.0 | S | Later, optional |
| n12 | Quiet by default | Request words, accept once, a team policy from the base branch | 6 | 3 | 6 | 5.0 | M | Request words before the public launch; accept once next |
| n20 | PR proof Action | Maintainer CI checks that new tests fail before and pass after | 6 | 5 | 3 | 4.9 | L | Later |
| n09 | Hand-forward | The next session starts with the open facts | 5 | 3 | 7 | 4.8 | S | No |
| n21 | Verify the CI agent | A step after claude-code-action that checks its run | 5 | 4 | 5 | 4.7 | M | Part of item 7 |
| n17 | AST rule core and rule packs | Rules on syntax trees, and community rule packs with fixtures | 5 | 3 | 4 | 4.1 | M | Later, if needed |
| n19 | Trap Kit | Test your agent setup against planted cheats | 4 | 5 | 2 | 3.9 | L | No (fixtures only) |
| n22 | Merge gate | Fix or accept each finding, and route reviews | 4 | 2 | 3 | 3.1 | M | No |
| n23 | Team ledger | Audit records in git, and rule health | 3 | 3 | 2 | 2.8 | M | No |

### Where I differ from the judges

- **n01** has low differentiation (3), because rivals already parse test output. I still put it first. Without it, the main promise of Agentproof is false.
- **n04** got 5.9 for the full scale of proof levels. Only its verdict part is urgent. The levels above level 1 need the proof run first.
- **n11** has the lowest differentiation (2). I rank it fifth, because it is the cheapest way to measure precision. It also shows value in the first minute.
- **n18** had only a screening. Its narrow form, the project's own precision numbers, is necessary for trust. I rank it sixth.
- **e01** has differentiation 3. I rank it ninth, because a second vendor is the main defense against a native Anthropic feature.
- **e02** keeps its score, but I change the idea. Build the reader, not the wrapper.
- **n06 (5.6) and n08 (5.8)** have good scores, but they wait. n06 accuses the agent, and n08 blocks a push. Both need precise findings first.
- **The judges disagreed on n02.** The value judges said "maybe", because honest work can look like a cheat. The differentiation judges said "strong". The strict gates in item 3 below answer both.

### The top 10

**1. Test forensics (n01). Do it first.**
- *Why do it:* It makes the main promise true. On 4 November 2026, one measurement read all Claude Code transcripts on the founder's computer. 495 of 525 test commands (94%) piped their output without pipefail, so the exit code was not the test result.
- *The sample:* 15 sessions in 4 projects, from 21 October to 3 November 2026. In the 3 real projects, the share was 89% (249 of 279), 100% (132 of 132) and 100% (83 of 83). The most common pipe targets were grep (271) and tail (218).
- *Definitions:* A test command is a Bash call in which one pipeline stage starts with a known test runner, also inside `docker compose run` or `sh -c`. "Piped" means that the output of the runner goes into `|` and the command has no `pipefail`. Each tool call counts one time.
- *Other figures:* Earlier judge samples gave 37% to 99%, because their definitions differ. #104504 measured 73% (1,514 of 2,084 runs) in one team. One judge found 124 runs that the engine calls "passed" with failure text in the output. In #104504, most unbacked claims were stale passes (32 of 36), not lies about a failed run.
- *How it works:* Keep the first 2 KB and the last 8 KB of the output of each test command, also in subagents. Mark a run "exit code hidden" when it pipes into tail, head or grep without pipefail. Parse the summary lines of pytest, jest, vitest, cargo and go first. Treat "0 tests", "no tests ran" and "collected 0 items" as "not run". Do not trust exit 0 from a background command until its summary line appears (#90659). A later pass clears a failure only when it ran the same tests or more.
- *Main risk:* Honest partial claims look like false claims, for example "the new lock test passes". Set the severity from the scope of the claim. Do not let strict mode block on these findings until the replay shows almost no false Critical findings.

**2. Honest verdicts (n04, verdict part only). Do it together with item 1.**
- *Why do it:* A trust tool must never say "safe" by mistake. Today, "safe to merge" can come after failures that the engine did not see. A transcript that the engine cannot read gives "no red flags found".
- *How it works:* Add a health gate that gives "incomplete" with a reason. Hide "safe to merge" until an executed proof exists. Count an AI note only when it cites a real transcript line or diff line. For each changed file, show the last change and the test result after that change.
- *Main risk:* The best verdict becomes "no red flags found", which feels weaker. That is the honest result. The proof run (item 10) can give a stronger verdict later.

**3. Test-tampering chain (n02). Highest score (6.4). The first new feature.**
- *Why do it:* It is the most differentiated idea (7 of 10). No tool in the research links "test failed, test edited, test passed" with the old and new expected values as evidence. On ImpossibleBench, Claude models cheated mostly by test edits.
- *Correction after the workflow:* WST2002/proof-of-done already blocks skipped or focused tests, weakened or always-passing assertions, deleted tests, hidden errors and lowered coverage thresholds. It reads them from the git diff since the session start. It does not catch an exact value that the agent changed to the value from the failure. So build two parts: the common forms, to match proof-of-done, and the copied-value chain, as the unique catch.
- *How it works:* Take the failing test and its expected and received values from the parsed output. Find the edits between that failure and the next pass. Give High only when all of these are true:
  - the test existed at the session start;
  - it failed before any edit to source code;
  - only the test or the test settings changed;
  - the failure was an assertion;
  - the final message did not tell the user about the change.
- *Main risk:* Honest work often has the same order of events. In the founder's sessions, 10 of 100 windows from a failure to a pass had edits only in tests, and none was a cheat. Keep the gates strict. Show the other cases as one "examine" line with the values.

**4. Lost data (n07, first part).**
- *Why do it:* It answers a question that only a list from the session start can answer: "What did the agent destroy that git cannot restore?" Today, `rm -rf data` on a gitignored database gives no finding.
- *How it works:* At session start, list untracked and ignored paths with their size and time. Use `--directory` so that each ignored folder gives one line. Stop at 300 ms or 50,000 entries, and mark the list "partial". At stop, find the paths that existed at the start and are now gone or empty. Give Critical only for valuable data: databases, uploads, untracked source and uncommitted changes. Skip temporary files such as logs, locks and SQLite -wal files.
- *Main risk:* Many ignored files go away for good reasons. Use a skip list and strict severity rules. Leave the backup copy and the prompt before deletion for later. Claude Code auto mode and /rewind already do part of that work.

**5. History audit (n11). I rank it above its score.**
- *Why do it:* It gives value in the first minute, from sessions that the user already has. It is also the founder's tool to measure precision on hundreds of real runs before launch.
- *How it works:* `agentproof audit --days 30` replays local transcripts with the rules that need no start point. It prints counts with labels such as "checked from output" or "exit code only", and links to sessions. It also shows a count of the items that it could not check. It shows the sessions that used Superpowers verification-before-completion apart, so users see what the skill misses. Later, it shows counts before and after the user fixes a habit.
- *Main risk:* show-your-work and proof-of-done already have a similar audit. Add facts from git for each commit to make it different. Example: "9 of 23 agent commits had no passing run after the last edit".

**6. Published accuracy (n18, narrow form). I rank it above its score.**
- *Why do it:* For a checker, trust in the checker is the product. ablanchard-dev/claimcheck publishes 0 false alarms on 8,707 turns. Agentproof publishes nothing yet.
- *How it works:* Build a labelled set from the founder's sessions, made-up "bad agent" sessions and ImpossibleBench-style cases (MIT license). Each rule needs at least 30 labelled positives. Fill rare rules with synthetic fixtures, and report real and synthetic results apart. A rule with fewer positives is "unmeasured" and cannot give Critical. Publish precision and recall for each rule in the README. Run the set in CI for each release.
- *Main risk:* The events that drive Critical findings are rare. The judges of idea e09 found 0 false test claims in 7 real sessions. Keep the set small and versioned, state its limits, and plan its maintenance. Do not score competitors at first, because a ranking made by one vendor looks biased.

**7. A CLI that reads session files, with exit codes (e02, narrow form).**
- *Why do it:* CI, scripts and other agents need exit codes. Agents already use the exit-code format of the CodeRabbit CLI, so copy it. It also covers headless runs that skip plugins. The Claude Code docs recommend `--bare` for scripted and SDK calls, and say that it will become the default for `claude -p`. Bare mode loads no hooks and no installed plugins (code.claude.com/docs/en/headless).
- *Where the dangerous-command rule helps:* Headless runs have no person who watches. A script can allow all Bash commands or use bypass mode, and then no classifier examines the commands.
- *How it works:* Add `agentproof check --session <file> --since <commit> --fail-on <level>`. Use exit code 0 for a pass and 2 for findings at or above the level. Use exit code 1 for a failed or incomplete check. Write a versioned JSON file and a GitHub job summary. claude-code-action gives an execution file and can load plugins (judge note, to confirm).
- *Main risk:* A CI gate with no person in the loop turns false alarms into blocked merges. Set the default level to Critical, and make the rules precise first.

**8. Narrow claim checks (n05, narrow form).**
- *Why do it:* It checks more than tests: "build OK", "lint OK", "type check OK", "pushed" and "no new dependencies". It also finds stubs that the agent did not mention, such as `todo!()` in a function that the final message calls "implemented".
- *How it works:* Keep the end of the output of build, lint and type-check commands. Check each claim against the last run after the last edit. Flag a stub only when the final message says "done" and names that file. Put each sentence that the rules cannot check in a "not checked" list.
- *Main risk:* Final messages are long and vague, so a false "contradicted" is easy. Give "contradicted" only for a direct mismatch. First, measure precision on the 1,047 turn-end messages in the founder's local sessions.

**9. Codex as the second agent (e01, Codex only). I rank it above its score.**
- *Why do it:* One vendor cannot easily replace a neutral checker that works with two vendors. Codex hooks send the same fields as Claude Code, so the start point and strict mode work almost without change. The dangerous-command rule also adds value for agents other than Claude Code.
- *How it works:* Add an adapter layer and a coverage record for each rule. Use only the SessionStart and Stop hooks. Parse Codex session files against real sample files from each Codex version. Fail loudly on unknown lines, and mark rules that cannot run as "not checked".
- *Main risk:* The Codex hooks docs say that the transcript format is not a stable interface for hooks and can change (learn.chatgpt.com/docs/hooks). Exit codes can also arrive late. Leave Cursor for later, because its shell hook has no documented exit code.

**10. Proof run, first version (n03).**
- *Why do it:* It changes "the transcript says" into "we ran it". Only Agentproof can do this part. When the transcript shows "failed, test edited, passed", it runs the original test against the new code.
- *How it works:* At first, run it only on `/agentproof:prove`. Build two clean copies outside the repository: the start state and the end state. Run the agent's own test command in both copies. Report only the differences: proven new tests, regressions, and runs that could not run.
- *Main risk:* A clean copy is not the developer's environment, so false failures are likely. Tally already runs tests at the base commit, so ship the unique part first. Store the start content of changed files. Never wake the agent when the environment fails.
- *Windows test plan:* The founder and the 3 real projects use Windows, but the first version runs on Linux and macOS. The sandbox runtime (srt) is alpha on Windows, Schannel HTTPS fails, and its profile grant exposes .npmrc tokens. Test the proof run in a GitHub Actions Linux runner and in WSL2 with the founder's 3 projects. On native Windows, show "proof run is not available on Windows yet". Never use srt without `--skip-profile-grant`.

**Next candidates:** "You said don't" (n06), the push gate (n08), dependency evidence (n16), accept once (n12) and the evidence card (n24). Each one needs precise findings first. The research ranks secret leaks #4 by pain and detectability (score 16), and GitGuardian reports about 2 times more leaks in commits from Claude Code. So the Next phase adds one new rule: a secret value added in the diff (High). Section 7 has the details.

## 6. Ideas from CodeRabbit-like tools

| Decision | Idea | Seen in | What Agentproof does |
|---|---|---|---|
| Copy | Fixed output for machines: JSON events, a severity gate, exit code 2 at the gate, exit code 1 on failure | CodeRabbit CLI (`--agent`, `--gate`) | The same exit codes with `--fail-on`, and a versioned JSON schema |
| Copy | Show stored findings again, and clear them | CodeRabbit CLI (`cr review findings`, `--clear`) | `agentproof findings` and `agentproof accept` |
| Copy | Free for public repositories, forever | CodeRabbit, Sourcery | All rules are free for everyone, always |
| Copy | A plugin inside the agent | CodeRabbit and OpenAI plugins for Claude Code | A listing in the official directory; a Codex bundle later |
| Copy | Review rules only from the base branch | Anthropic Code Review (REVIEW.md) | Read a team policy file only from the default branch |
| Copy | Few findings by design | Codex on GitHub (P0 and P1 only); /code-review `--max-findings` | The message shows at most 3 items. The report shows all items |
| Adapt | Learnings ("remember this") | CodeRabbit learnings; Cursor "@cursor remember" | Local "accept once" fingerprints, with no cloud. The finding comes back when its evidence changes |
| Adapt | A verification pass before posting | Claude Code Review | An AI note counts only when it cites a real transcript line or diff line |
| Adapt | Effort levels | Copilot Lite/Balanced/Max; Claude Quick/Standard/Deep | A fast check after each turn, and a proof run on request |
| Adapt | Blast Radius | CodeRabbit Advanced | Keep the blast radius table, with facts from git and the transcript only |
| Adapt | Hand-off to an agent for fixes | CodeRabbit IDE extension | Strict mode gives the facts to the agent one time. Agentproof makes no automatic fixes |
| Adapt | Checks before the merge | CodeRabbit pre-merge checks | Gates on facts only: `--fail-on` in CI, and a push gate later |
| Adapt | A monthly leaderboard for open source | CodeRabbit | A monthly post of real catches from users who agree to share them |
| Avoid | AI review of the diff for bugs | All diff reviewers | Leave it to them. Many tools already do it, and this review does not use the transcript |
| Avoid | A hosted GitHub App with write access | CodeRabbit, Bugbot | Stay local. A 2025 CodeRabbit bug leaked a key with write access to about 1M repositories |
| Avoid | A price for each review or each file | Greptile, Copilot, the CodeRabbit add-on | Users dislike this model (the Greptile pricing complaints) |
| Avoid | Agents that push fixes | Bugbot Autofix, "@codex fix" | These break "never touch the repo" and "the checker is never the agent" |
| Avoid | An AI gate that blocks on opinion | codex-plugin-cc review gate | Its README warns of loops that use up the plan. Block only on facts |
| Avoid | Chat with the PR, diagrams, summaries | Many reviewers | Others do this well, and it does not use the transcript |
| Avoid | Rules from the PR branch | Copilot code review | A pull request can then change its own review rules |

## 7. Roadmap

Effort sizes are for one founder who works with Claude Code. S is 1 to 2 days, M is 3 to 5 days, and L is 6 to 10 days. Each phase holds about 4 weeks of work. If a phase runs late, move the soft launch. Do not cut the tests.

**Performance budget.** The principle "never slow the agent down" applies to all new work. SessionStart work must add less than 300 ms on the largest of the 3 real projects. The stop check must stay under 2 s on the largest real session. Each new feature has a test for its budget.

### Now, Phase A: weeks 1 to 4 (P0 fixes and their regression tests only)

| Week | Work | Effort | Done when |
|---|---|---|---|
| 1–2 | **Read test output (n01 core; blockers 1 and 8).** Keep the first 2 KB and the last 8 KB of the output of each test command, also in subagents. Mark piped runs "exit code hidden". Parse pytest, jest, vitest, cargo and go summaries. Add the missing test-command forms (`npm run test:*`, `bun run test`, `python manage.py test`, `nx test` and others). Treat zero-test runs as "not run". Do not trust exit 0 from a background command until its summary appears | L | Experiments e1 and e7 give "do not trust this work". A "tests pass" claim after `npm run test:unit` gives no finding. No report says "the last test run passed" for a piped run. The stop check stays under 2 s on the largest real session |
| 2 | **Scope of runs (n01; blocker 2).** A later pass clears a failure only when it ran the same tests or more | S | Experiment e2 gives "do not trust this work" |
| 2–3 | **Honest verdict (n04, verdict part; blockers 3 and 4).** Add the health gate. Hide "safe to merge". Send the end of the output to the AI check. Check the AI citations | M | Experiments e3 and e3b give "incomplete". No path gives "safe to merge" from the AI check alone |
| 3 | **Secrets (blocker 5).** Remove secrets from each saved field. Add key patterns. Fix the order of the secret-file check | S | Experiment e6 finds no live key in the JSON file, the report or the AI request |
| 3 | **Attribution (blocker 6).** Do not lower Critical, High, CI or secret findings to Low as "other origin". Treat commands on folders as edits to the files in them | S | Experiment e4 gives "do not trust this work" |
| 4 | **Ignored data (blocker 7).** At session start, list ignored and untracked paths with `git ls-files --others --directory`, so that each ignored folder gives one line. Stop at 300 ms or 50,000 entries, and mark the list "partial". Remove the cleanup exemption for old data | M | Experiment e9 gives a Critical finding for the deleted database. SessionStart adds less than 300 ms on the largest of the 3 real projects |
| 4 | **Regression suite.** Add each audit experiment (e1 to e9, e3b) as a fixture test | S | Each experiment runs as a test, and all tests pass |

### Now, Phase B: weeks 5 to 8 (install, CI, license, README, audit, then soft launch)

| Week | Work | Effort | Done when |
|---|---|---|---|
| 5–6 | **Binaries (blocker 9).** Use cargo-dist for Windows, Linux and macOS (x64 and arm64), with checksums. Put the binaries in the plugin release, so that no download happens at run time. Document the macOS quarantine step and the Windows SmartScreen warning until you sign the binaries | M | A GitHub Actions macOS runner and a Windows runner install from the release and give a verdict on a fixture session. The founder has no Mac, so the macOS runner is the test |
| 5 | **Windows launcher (blocker 9).** Ship a PowerShell launcher next to the bash launcher. Claude Code runs hook commands in Git Bash on Windows, and in PowerShell when the computer has no Git Bash. Make the hook command start the correct launcher in each shell. Or use the exec form, which starts a real .exe with no shell | S | The e2e script passes on a Windows runner with no Git Bash on PATH |
| 6 | **License, CI, security policy (blocker 10).** Add LICENSE-MIT, LICENSE-APACHE and SECURITY.md. Run cargo test, clippy and the e2e scripts in CI | S | CI passes on 3 systems for each pull request |
| 6 | **Safe AI defaults (blockers 11 and 12).** Never wait for the AI check in a hook, also in strict mode. Remove agentproof-ai from the default install. Remove the false README claim about the next message | S | No hook waits for an AI call. The README has no false AI claim |
| 7 | **Data on disk (blocker 15).** Add `agentproof clean --older-than 30d`, a 30-day default limit and a setting that leaves the full session out of the JSON file. Add the "Data on disk" paragraph to the README | S | Session folders older than 30 days go away. With the setting on, the JSON file has no session text |
| 7 | **First run and feedback.** Add `/agentproof:doctor` (health check) and `/agentproof:wrong` (false-alarm report). Add issue templates for false alarms and missed failures | S | On a clean runner, the first message confirms that both hooks ran. `/agentproof:wrong` writes a redacted local file |
| 7–8 | **History audit, first version (n11), and the labelled set.** Replay the founder's transcripts. Label a sample by hand. Add synthetic fixtures for rare rules | L | Each rule that can give Critical has at least 30 labelled positives, or it has the mark "unmeasured". The README shows the precision of each rule, with real and synthetic results apart |
| 8 | **Precision gate.** A rule with low or unmeasured precision cannot give Critical and cannot block | S | No rule with measured precision under 90%, and no unmeasured rule, can give Critical or block in strict mode |
| 8 | **Name and README.** Check the name on GitHub, npm, PyPI, domains and trademarks. Rewrite the README with the new positioning and no false claims | S | No claim in the README is false according to a test or an audit experiment |
| End of 8 | **Soft launch.** Make the repository public. Submit to the Claude plugin directory and to awesome-claude-code. Use Agentproof on its own development each day | S | You submitted the directory listing, and you saved 5 real catches as redacted screenshots |

### Next: months 3 to 5

| Item | Effort | Done when | Priority |
|---|---|---|---|
| AI check repairs and cost rules (blockers 11 and 13). Show the latest finished AI result. Test with real keys. Run the call only on turns that changed files and make a claim. Show the cost of each call | M | Real-key tests pass for Anthropic and one OpenAI-compatible server. The report shows the latest AI result and its cost | Must, before the public launch |
| Requested-work noise (blocker 14; n12, first part) | M | Experiment e5 gives only Low findings, each with the note "You asked for this" and the prompt line | Must, before the public launch |
| Destructive patterns, with the Windows forms (blocker 16) | M | Experiment e8 and the fixture for each new pattern give a finding. A force push that the user asked for gives Low | Must |
| Secret value added in the diff (new rule, High). Use gitleaks-style rules plus entropy on added lines only, with the same pattern set as redaction and validation off. Also flag `cat .env` and similar commands that print secrets into the transcript | M | Fixtures with planted keys give High. The founder's real sessions give no false High | Must |
| Test-tampering chain with the strict gates (n02) | M | It catches each tampering fixture and gives zero High findings on the founder's real sessions | Must |
| Lost-data finding (n07, first part) | M | Deletion fixtures give Critical. Real sessions give no Critical finding for temporary files | Must |
| A CLI that reads session files, plus a CI step (e02 and n21) | M | A sample workflow runs Agentproof after claude-code-action and fails the job on a Critical finding, with the reason in the job summary | Must |
| Public launch in month 4: one post and Hacker News | S | You published the post with real catches and precision. The directory listing is live, or the self-hosted install works | Must |
| Narrow claim checks (n05) | M | Each claim type has 90% or more precision on labelled turn-end messages | Must |
| Codex adapter (e01, Codex only) | L | 20 real Codex sessions give verdicts. Unknown lines give "incomplete", never "no red flags found" | Must |
| Code signing: Apple Developer ID with notarization, and Windows Authenticode | S | A binary from the release starts with no warning on clean macOS and Windows runners | Should |
| Proof run, first version, on request (n03) | L | `/agentproof:prove` proves or disproves each new test on the founder's 3 projects in WSL2 and on a Linux runner. It keeps "could not run" apart from "failed". Native Windows shows "not available yet" | Should |
| Accept once (n12, second part) | S | An accepted finding stays silent until its evidence changes | If time permits |
| Dependency evidence, offline part (n16) | M | Each new package shows by name, with install errors and the typo distance to popular names | If time permits |
| Push gate, opt-in (n08) | S | A push, PR creation or merge with an open proven Critical finding asks the user one time, with the evidence | If time permits |

### Later: months 6 to 12

| Item | Done when |
|---|---|
| Proof run in the background (asyncRewake hook), and proof levels in the verdict | "Safe to merge" comes back, but only after an executed proof. The verdict names its proof level |
| PR proof Action (n20), free for public repositories | 10 public repositories use it |
| Cursor (preview) and OpenCode. Readers for Entire checkpoints, Agent Trace and ATIF | Each agent has a coverage table. No agent gets "no red flags found" when a critical rule cannot run |
| "You said don't", as a thin layer on the existing findings (n06) | Zero false alarms on a labelled set of real prompts |
| Local history, with counts before and after a change of habit | The audit shows the change after the user fixes a habit |
| A team policy file, read only from the default branch (n12, third part) | A policy change inside a pull request has no effect and gives a finding |
| Evidence card and repro bundle (n24) | A user can share a redacted card for a Critical or High finding |
| More security checks: secret flow with no tripwire (n15), oversight drift (n14), research on injection chains (n13) | Each one has a labelled fixture set and published precision before release |
| Open dataset | The labelled set is public, and at least one other tool reports results on it |
| Business tests: sponsors first, and a paid CI option only if the Action has users | Sponsor income or a first paying team, with no change to the free core |

## 8. What not to do

- **Do not build an AI diff reviewer (a CodeRabbit clone).** Many tools already do it, and it does not use the transcript, which is your unique data.
- **Do not launch on Hacker News until you fix the false passes, the install works and you publish precision.** You get one main launch.
- **Do not give "safe to merge" from an AI opinion.** A trust tool that says "safe" by mistake loses all trust.
- **Do not ship agentproof-ai in the default install** until it passes tests and its result reaches the report.
- **Do not add many agents before the Claude Code checks are accurate.** Add Codex only, after the core is correct.
- **Do not build an agent scorecard, an Agent Honesty Index or agent duels.** The events are rare, the task mix is different for each agent, and nobody can check the numbers.
- **Do not create a new open standard now.** Nobody will use a format from a tool with no users. Publish a JSON schema, and read the existing formats.
- **Do not add telemetry, an account or a server.** Use the directory Usage tab and the GitHub Releases download count for numbers.
- **Do not fix code automatically or write to the repository.** This breaks two core principles.
- **Do not block the agent on an AI opinion or on rules with low precision.** Block only on proven Critical findings, and only one time.
- **Do not lead with dangerous commands.** Native features and guards stop most of them earlier. Mention the rule for bypass mode, headless runs, CI and agents other than Claude Code.
- **Do not prompt before each command.** One opt-in gate asks only before push, PR creation or merge while a proven Critical finding is open. Auto mode misses many risky actions, so Agentproof gives the audit after the turn.
- **Do not run the AI check on each turn by default.** Run it only on turns that changed files and make a claim, and show its cost.
- **Do not move rules behind a paid license later.** Semgrep did this, and the community made a fork (Opengrep).
- **Do not bundle code with restrictive licenses.** Examples: Semgrep registry rules, CodeQL and TruffleHog (AGPL). Do not copy code from ablanchard-dev/claimcheck (GPL) or Vet (AGPL).

## 9. Community and growth plan

### Launch sequence

1. **End of week 8: soft launch.** Make the repository public with a one-line install, real catches and precision numbers. Submit Agentproof to the directory and to awesome-claude-code.
2. **Month 3: one technical post.** Title idea: "Your agent says the tests pass. Here is what N real runs show." Use the #104504 data, ImpossibleBench and your own measurement (94% of 525 test commands piped). Say which data is yours, and say how small it is.
3. **Month 4, after the tampering chain is in a release and you fixed the P1 items: Hacker News.** Post between 12:00 and 17:00 UTC with a plain title. In the study, the "Show HN" tag and the day of the week had no clear effect.
4. **Answer where people already ask.** Issues #104504 and #103500 discuss this exact problem, and both are open. A bot closed and locked #39945. Reply only where Agentproof solves the problem, and reply one time only.
5. **Ask for reviews.** Contact people who already wrote about agents that change tests. An example is the author of the blog.fsck.com post about deleted tests.

### Onboarding: the first session

- **Install.** One command adds the marketplace, and one command installs the plugin. The README states the requirements: Git, and on Windows Git Bash until the PowerShell launcher ships.
- **New session.** Claude Code loads a plugin only when a session starts. The install text tells the user to start a new session.
- **First message.** The SessionStart hook prints nothing, because its output goes into the agent's context. The first Stop message confirms that both hooks ran. It gives the report path and the state of three settings: AI check off, strict mode off, proof run not available yet.
- **Health check.** `/agentproof:doctor` shows the engine version, the launcher and the shell. It confirms that the start hook ran in this session and that the engine can read the transcript. If the start hook did not run, it tells the user to start a new session.
- **History audit.** After Phase B, the first message suggests `/agentproof:audit` for the last 30 days. This gives value on the first day.
- **AI check.** The docs show how to turn it on in the user settings file. They show the estimated cost per call and per day for each model (section 10).
- **Strict mode.** The docs explain that strict mode stops the agent one time for each new proven Critical finding.
- **False alarms.** `/agentproof:wrong` writes a redacted local file. The user can attach it to the "false alarm" issue template.

### Distribution channels

| Channel | Why | Action | When |
|---|---|---|---|
| Claude plugin directory (528 plugins) | The main channel, inside Claude Code. The Usage tab shows installs, retention and hook runs with no telemetry | Submit at claude.ai/directory/manage. Answer the data questions honestly: data leaves the computer only when the user turns on the AI check | Week 8 |
| Self-hosted marketplace | No review delay | Publish `claude plugin install agentproof --marketplace <owner>/<repo>` (Claude Code 2.1.292+). Tell users to turn on auto-update, which is off by default | Week 8 |
| GitHub Releases with cargo-dist | The base for the plugin, the CLI and CI | Shell, PowerShell, Homebrew tap and MSI installers. Document the macOS quarantine step and the SmartScreen warning until you sign | Weeks 5 and 6 |
| awesome-claude-code (57.2k stars) | A focused audience and a badge | Follow its CONTRIBUTING.md process | Week 8 |
| GitHub Marketplace (Action) | Reaches teams | Publish the CI step | Months 3 to 5 |
| Codex plugin or hooks bundle | A second audience | Ship it with the Codex adapter | Month 5 |
| Homebrew core | Wide reach on macOS | A self-submission needs 225 stars, 90 forks or 90 watchers, and a repository older than 30 days | Later |

Put the binaries in the plugin release, so that no download happens at run time. If a download becomes necessary, pin the version and check the checksum or signature. Also print one line that says what the plugin downloads. Declare the download in the data-handling form of the directory.

### Open standard

- Do not create a new standard now. Entire and Agent Trace already cover parts of this area.
- Publish a versioned JSON schema for the check result. It holds the verdict, the findings, the evidence and the rules that could not run.
- Later, read existing formats as input: Entire checkpoints, Agent Trace and ATIF.
- Publish the labelled set as an open dataset, so that other checkers can test on it. People can use this on the first day, so it can act as a standard.

### Content

- A page of real catches with redacted evidence, only from users who agree.
- An update of the accuracy numbers with each release.
- One short post for each type of failure, with sources.
- A 30-second screen recording of one real catch.
- STE100 text as a feature: it is clear for readers whose first language is not English.

### Contributors

- Make each contribution a fixture plus a test. Examples: one parser for a test runner, one destructive pattern, or one form of a test command.
- Label good first issues, and reply within 2 days. In popular projects, the merge rate of these pull requests from newcomers fell from 61.9% to 42.2%.
- Release the parsers for test output as a separate crate under the MIT license.
- Add issue templates for false alarms and missed failures. Each template asks for the redacted file from `/agentproof:wrong`.
- Write the contributor guide in STE100.

### Targets

- **The founder's current targets.** The project description file ([project-description.md](project-description.md)) sets them. Launch week: the front page of Hacker News or 500+ GitHub stars, and 5 real catches as screenshots. 3 months: 1,000+ stars, 10+ outside contributors and a second agent.
- **The study.** In the study of 138 AI repositories, a Hacker News launch gave a mean of 289 stars in 7 days. The median was much lower. A target of 500 stars is above the typical result. Use installs and 30-day retention as the main targets, and stars as a second measure only.
- **North-star metric: 30-day active installs.** This is the number of installs that ran a check in the last 30 days. Users keep a checker on only when it catches real problems with few false alarms. So this one number shows value and trust together.
- **Measurement with no telemetry.** The directory Usage tab shows installs, retention and hook runs. Self-hosted marketplace installs give no Usage tab data. So before the listing is live, count the downloads of GitHub Releases. For false alarms, count the `/agentproof:wrong` reports and the issues. Reports undercount, so use them for trends only.
- **Verified catch.** A verified catch is a High or Critical finding that a person confirmed as true.

I propose these values as starting targets. Change them after the first month of data.

| Metric | Source | Month 3 | Month 6 | Month 12 |
|---|---|---|---|---|
| **30-day active installs (north star)** | Directory Usage tab | 60 | 350 | 1,400 |
| Installs | Directory Usage tab | 200 | 1,200 | 4,000 |
| Downloads of GitHub Releases (the measure before the listing is live) | GitHub API | 300 | 1,000 | 3,000 |
| Precision of each rule that can give Critical | Labelled set: at least 30 positives for each rule, real and synthetic results apart | 90% or more | 90% or more | 95% or more |
| Verified catches per 100 sessions | History audit of the founder's sessions, and audits that users share | Publish the first value | Publish with each release | Publish with each release |
| False-alarm reports per 100 sessions | `/agentproof:wrong` reports and issues, divided by SessionStart hook runs | Measure | Under 5 | Under 2 |
| Real catches that users share | Issues and posts | 5 | 20 | 50 |
| Supported agents | Releases | Claude Code | Claude Code, Codex and the CI step | Claude Code, Codex, the CI step and one more agent |
| Outside contributors | GitHub | 3 | 10 | 20 |

If the directory listing is not live by month 3, use the downloads of GitHub Releases instead.

## 10. Business model options

Expect no revenue in the first 6 to 12 months. The goal now is adoption and trust.

| Option | How it works | Core stays local, free and private? | When |
|---|---|---|---|
| Free core, forever | All rules, the report, the plugin and the CLI stay free (MIT OR Apache-2.0). Say this in the README now | Yes | Now |
| Sponsors and grants | GitHub Sponsors. Companies that make AI tools can sponsor: CodeRabbit and Blacksmith sponsor ccusage. Cline gives grants of $1k–$10k in credits | Yes | After the public launch |
| Paid CI option for private repositories | Free for public repositories. Paid plans through GitHub Marketplace billing, like pre-commit.ci ($10, $20 or $100 per month) | Yes, if transcripts stay on the runner | Only after the Action has users |
| Team pack | Shared policy, history for a team, audit export. Self-hosted, with a license key | Yes | After 12 months, if teams ask |
| Support and custom rules | Paid support. Rule packs made for one company | Yes | When a company asks |
| Sponsorware (early access for sponsors) | Sponsors get new features first | Partly | We do not recommend it. It needs many sponsors, and the best-known example ended in November 2025 |
| Acquisition or partnership | Labs buy verification tools: OpenAI bought Promptfoo and Astral in 2026 | Not a plan | Keep the code and the licenses clean |

Do not use these four models. They are a price for each review, a required account, telemetry and a later change to the rule license.

### Cost of the optional AI check

The AI check is off by default. When a user turns it on, the default model is `claude-opus-5-5` at medium effort. Today the call runs after each turn with a new final message, when any file changed in the session.

**Input size.** The estimate applies the request builder in `engine/src/ai.rs` to the 9 saved checks of the 3 real projects. It uses the last turn of each session. Line counts give the size of the diff text. The estimate uses about 3 characters per token.

| Input of one call (last turn of the session) | Tokens |
|---|---|
| Smallest | 1,600 |
| Median | 26,000 |
| Largest | 57,000 |

The engine limits the diff to 120,000 characters, and 3 of the 9 requests reached that limit. Nobody has measured the output yet. The table below uses 2,000 to 6,000 output tokens, including the thinking of the model.

**Price per call and per day.** The prices are Anthropic's published prices per million tokens on 4 November 2026. The founder had a median of 55 turns on each active day (788 turns in 14 days; the busiest day had 84). In 41% of turns (324 of 788), the agent used a file-edit tool.

| Model | Input / output price | One median call | One large call | Median day, a call on each turn (55) | Median day, only turns with edits (about 23) |
|---|---|---|---|---|---|
| `claude-opus-5-5` (default today) | $4 / $20 | $0.14–$0.22 | $0.27–$0.35 | $8–$12 | $3.30–$5.10 |
| `claude-sonnet-5-5` | $2 / $10 | $0.07–$0.11 | $0.13–$0.17 | $3.90–$6.10 | $1.60–$2.60 |
| `claude-haiku-5-5` | $0.10 / $0.50 (prompts up to 100,000 tokens) | $0.004–$0.006 | $0.007–$0.009 | $0.20–$0.31 | $0.08–$0.13 |
| OpenAI-compatible provider | The published price of the provider | Same token counts | Same token counts | – | – |
| Local model, for example through Ollama | No money cost | – | – | – | – |

On the busiest day, `claude-opus-5-5` with a call on each turn can cost about $29. Cost for each review is the main complaint in this category, so keep the optional AI check small.

**Rules for the AI check:**
- Run the call only when the turn changed files and the final message makes a claim about the work.
- Show the estimated cost per call and per day in the report and in the config docs. After each call, show the real cost from the token counts in the answer.
- Add a daily cost limit to the user settings. When a call would go above the limit, skip it and say so in the report.
- Test `claude-haiku-5-5` and `claude-sonnet-5-5` on the labelled set. Make the cheapest model with the same precision the default. Keep `claude-opus-5-5` as an option.
- The agentproof-ai plugin costs no money, but it uses plan limits on each turn. Measure that use before you recommend the plugin.

## 11. Risks

| Risk | Likelihood | Impact | What to do |
|---|---|---|---|
| Anthropic builds a native claim check. It already has /goal, agent hooks, /insights, /verify and Code Review | High | High | Support Codex and CI, where a feature of one vendor cannot go. Keep evidence that a prompt cannot give: test output, git since the start, executed proof. Use the native hooks: ConfigChange and InstructionsLoaded for the sensitive-path rule, PostToolUseFailure for the repeated-error rule, SubagentStop for subagent claims. Trigger: if Anthropic ships a native claim check, the next release moves the main message to Codex support and the CI step. It then reads the native result as an input |
| Anthropic fixes the exit codes of piped commands in the Bash tool | Low | Medium | #104504 and #103500 are open. A stale bot closed #39945 as "not planned", with no maintainer decision. Do not make pipe detection the main message. Stale, narrowed and tampered tests still matter after a fix |
| False alarms make users turn Agentproof off | High | High | Use the precision gate for Critical findings (90% on at least 30 positives). Also use the labelled set, accept once and news-only messages. Block only on proven Critical findings |
| A change to the Claude Code transcript format breaks parsing | High | High | Use the health gate. Keep sample files for each Claude Code version in CI. Give "incomplete" on unknown lines |
| A funded rival (Entire) adds fact-based claim checks | Medium | High | Read Entire checkpoints as input. Stay local, free and with no account. Release complete, deep checks first |
| Small rivals copy single features | High | Medium | Build protection against copies from accuracy, depth, a working install and independence. Finish each feature |
| Agentproof itself leaks secrets | Medium | High | Remove secrets from all saved files. Test each pattern. List the removed items in the report. Let only the user settings file turn on the AI check. Delete session data after 30 days |
| Attackers misuse the engine download | Low | High | Put the binaries in the plugin release, so that no download happens at run time. If a download is necessary, pin the version and check the checksum or signature. Keep SECURITY.md |
| Gatekeeper or SmartScreen blocks the engine | Medium | High | Sign and notarize on macOS, and sign with Authenticode on Windows. Until then, document the quarantine step and the SmartScreen warning. Test the install on GitHub Actions macOS and Windows runners |
| Headless Claude runs skip plugins. The Claude Code docs say that `--bare` will become the default for `claude -p` | High | Medium | Use the CLI that reads session files (item 7) and the CI step |
| The AI check costs more than users expect | Medium | Medium | Run it only on turns that changed files and make a claim. Show the cost of each call. Add a daily limit. Test a cheaper default model |
| One founder cannot do all of this | High | High | Use the list of things not to do. Keep each phase to about 4 weeks of work. Ship one finished feature in each release. Rely on CI tests and on contributions with fixtures |
| License or name problems | Low | Medium | Add the license files. Check the name. Do not copy GPL or AGPL code |
| The proof run executes code from the repository | Medium | High | Make it opt-in in the user settings only. Ask for consent for each project. Use a sandbox with no network. Never turn it on by default |
| The directory review blocks the listing | Medium | Medium | Keep the self-hosted marketplace. Ask the directory team early. Declare all network use in the data-handling form |

## 12. Sources

**Diff reviewers and vendor review**
- https://www.coderabbit.ai/pricing
- https://www.coderabbit.ai/newsroom/reuters-coderabbit-valued-at-1-5-billion
- https://docs.coderabbit.ai/cli/agent-mode
- https://docs.coderabbit.ai/cli/claude-code-integration
- https://coderabbit.ai/blog/introducing-usage-based-add-on-allow-unrestricted-access-to-coderabbit-cli-through-agentic-coding-loops
- https://coderabbit.ai/blog/one-million-open-source-reviews-september-2026
- https://kudelskisecurity.com/research/how-we-exploited-coderabbit-from-a-simple-pr-to-rce-and-write-access-on-1m-repositories
- https://www.greptile.com/pricing
- https://www.usagepricing.com/blueprint/activity/greptile-2026-10-23-pro-seat-fee-dropped
- https://graphite.com/pricing
- https://www.qodo.ai/pricing/
- https://www.sourcery.ai/pricing
- https://docs.github.com/en/copilot/concepts/agents/code-review
- https://cursor.com/docs/bugbot
- https://code.claude.com/docs/en/code-review
- https://code.claude.com/docs/en/code-review-local
- https://code.claude.com/docs/en/ultrareview
- https://learn.chatgpt.com/codex/code-review
- https://github.com/openai/codex-plugin-cc

**Transcript tools, claim checkers and self-check skills**
- https://github.com/imbue-ai/vet
- https://github.com/entireio/cli
- https://docs.entire.com/guides/trails/overview.md
- https://techcrunch.com/2026/02/10/former-github-ceo-raises-record-60m-dev-tool-seed-round-at-300m-valuation/
- https://github.com/kenn-io/agentsview
- https://github.com/kenn-io/agentsview/blob/main/docs/session-intelligence.md
- https://github.github.com/gh-aw/experimental/trace-graders/
- https://github.com/kru3ish/tally
- https://github.com/Jasowills/receipts
- https://github.com/dheerajjha/show-your-work
- https://github.com/B0yko/proof-of-done
- https://github.com/WST2002/claude-code-skills
- https://github.com/ablanchard-dev/claimcheck
- https://github.com/veltiq/groundtruth
- https://github.com/vnmoorthy/groundtruth
- https://github.com/0xelitesystem/agent-receipts
- https://github.com/mattyopon/claude-fake-work-guard
- https://github.com/TychoHenzen/dod-guard
- https://github.com/obra/superpowers

**Native platform features and guards**
- https://code.claude.com/docs/en/hooks
- https://code.claude.com/docs/en/headless
- https://code.claude.com/docs/en/permission-modes
- https://code.claude.com/docs/en/goal
- https://code.claude.com/docs/en/checkpointing
- https://code.claude.com/docs/en/sandboxing
- https://github.com/anthropic-experimental/sandbox-runtime
- https://learn.chatgpt.com/docs/hooks
- https://github.com/Dicklesworthstone/destructive_command_guard
- https://github.com/kenryu42/cc-safety-net
- https://github.com/nizos/probity

**Evidence of agent failures**
- https://github.com/anthropics/claude-code/issues/104504
- https://github.com/anthropics/claude-code/issues/103500
- https://github.com/anthropics/claude-code/issues/39945
- https://github.com/anthropics/claude-code/issues/55340
- https://github.com/anthropics/claude-code/issues/90659
- https://github.com/anthropics/claude-code/issues/88462
- https://github.com/anthropics/claude-code/issues/45550
- https://arxiv.org/html/2510.20270v1
- https://arxiv.org/abs/2604.04978
- https://arxiv.org/abs/2605.18583
- https://www.digitalapplied.com/blog/claude-code-auto-mode-classifier-numbers-published
- https://blog.fsck.com/2026/04/30/that-time-it-tried-to-delete-all-my-tests
- https://gitguardian.com/state-of-secrets-sprawl-report-2026
- https://www.usenix.org/conference/usenixsecurity25/presentation/spracklen
- https://www.sonarsource.com/company/press-releases/sonar-data-reveals-critical-verification-gap-in-ai-coding/
- https://survey.stackoverflow.co/2025/ai
- https://www.theregister.com/2025/07/21/replit_saastr_vibe_coding_incident/
- https://www.theregister.com/2025/12/01/google_antigravity_wipes_d_drive/

**Growth, distribution, prices and business models**
- https://claude.com/plugins
- https://claude.com/docs/plugins/submit
- https://code.claude.com/docs/en/discover-plugins
- https://code.claude.com/docs/en/plugins/anthropic-marketplaces.md
- https://platform.claude.com/docs/en/about-claude/pricing
- https://github.com/hesreallyhim/awesome-claude-code
- https://github.com/ryoppippi/ccusage
- https://arxiv.org/abs/2511.04453v1
- https://github.com/axodotdev/cargo-dist
- https://docs.brew.sh/Package-Acceptance-Policy
- https://pre-commit.ci/
- https://github.com/marketplace/pre-commit-ci
- https://cline.bot/blog/5m-installs-1m-open-source-grant-program
- https://www.thestack.technology/semgrep-fork-opengrep/
- https://squidfunk.github.io/mkdocs-material/blog/2025/11/11/insiders-now-free-for-everyone/
- https://arxiv.org/abs/2604.27532
- https://github.com/promptfoo/promptfoo
- https://simonwillison.net/2026/Mar/19/openai-acquiring-astral

**Technologies for evidence**
- https://github.com/ast-grep/ast-grep
- https://github.com/gitleaks/gitleaks
- https://github.com/betterleaks/betterleaks
- https://github.com/google/osv-scanner
- https://github.com/rustfoundation/typomania
- https://mutants.rs/in-diff.html
- https://pypi.org/project/pytest-testmon/
- https://pypi.org/project/diff-cover/
- https://agent-trace.dev
- https://github.com/harbor-framework/harbor

**Internal inputs**
- The product audit from this workflow (experiments e1 to e9 and e3b against the built engine). [analysis/appendix.md](analysis/appendix.md) has the full audit.
- The judge notes for all ideas: [analysis/appendix.md](analysis/appendix.md).
- The project description, with the founder's targets: [project-description.md](project-description.md).
- The engine code: engine/src/checks.rs (the test-tool list), engine/src/check.rs (when the AI call runs) and engine/src/ai.rs (the AI request and its limits).
- The measurements of 4 November 2026 are Perl scripts in [analysis/measure](analysis/measure). Run them in Git Bash from the project folder, for example `perl docs/analysis/measure/pipes.pl`:
  - pipes.pl: piped test commands.
  - turns.pl: turns for each day.
  - editturns.pl: turns with edits.
  - aisize.pl: the size of the AI request. Give it the check.json files as arguments.
- https://github.com/WST2002/claude-code-skills/tree/main/plugins/proof-of-done (README and lib/tamper.mjs, read on 4 November 2026 for the correction).

## 13. Verification of this report

After the workflow, I checked these claims again on 4 November 2026.

| Claim | How I checked it | Result |
|---|---|---|
| A piped test run with failures gives "no red flags found" (blocker 1) | I made a new test repository and a new transcript: `npm test 2>&1 \| tail -5` with "1 failed" in the output, then "All tests pass". I ran the shipped engine on it | Confirmed. The verdict was "no red flags found", with exit code 0 |
| The engine keeps the output only of failed commands | engine/src/transcript.rs, lines 264 to 276 | Confirmed. For a command with no error, the engine sets exit code 0 and keeps no output |
| 495 of 525 test commands (94%) piped their output | I ran analysis/measure/pipes.pl again on all local transcripts | Confirmed: 495 of 525 (94.3%). The pipe targets were grep (271) and tail (218) |
| The results of experiments e1 to e9 and e3b | I read the saved engine output of each experiment | Confirmed. Each verdict agrees with section 4 |
| Live keys in check.json and report.html (blocker 5) | I counted the planted keys in the saved files of experiment e6. The audit made these keys for the test. They are not real | Confirmed. check.json has 2 Anthropic-style keys and 4 Stripe-style keys. report.html has 4 Stripe-style keys. last.json has 1 Stripe-style key. engine/src/redact.rs has no pattern for `sk_live_` |
| The model prices in section 10 | The Claude API reference (models.md) | Confirmed: Opus 5.5 $4 / $20, Sonnet 5.5 $2 / $10, Haiku 5.5 $0.10 / $0.50 for prompts up to 100,000 tokens |
| `--bare` will become the default for `claude -p`, and bare mode loads no hooks and no installed plugins | code.claude.com/docs/en/headless | Confirmed |
| Codex: the transcript format is not a stable interface for hooks | learn.chatgpt.com/docs/hooks | Confirmed |
| Auto mode is the default start mode from Claude Code 2.1.283 | code.claude.com/docs/en/permission-modes | Confirmed for interactive terminal and VS Code sessions |
| The founder's launch targets | project-description.md, section "What success looks like" | Confirmed |
| What WST2002/proof-of-done finds | Its README and lib/tamper.mjs on GitHub | Corrected. It also finds the common forms of test tampering. Sections 2, 3 and 5 now say this |

I did not check the other competitor facts again: star counts, prices and features. Five fact-checkers in the workflow checked them. Star counts change fast.