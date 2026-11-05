# Agentproof roadmap

Last update: 5 November 2026. Status of Phase A: In progress. A1 to A6 are done, and A7 is next.

The maintainer and Claude Code built milestones 1 to 4 of Agentproof on 3 November 2026. The product analysis of 4 November 2026 found that Agentproof is not ready for production use. Its main check gives false passes, and nobody can install it without a manual build. Phase A fixes the 8 P0 blockers in weeks 1 to 4, each with a regression test. The maintainer confirmed the start of Phase A on 4 November 2026. A1, the fix for the false pass, is done on the same day (see "A1 result").

## How to read this file

This file is the plan for the work. The maintainer and Claude Code continue the work from this file, one item at a time. It replaces the "Roadmap" section of [project-description.md](project-description.md). The plan comes from [product-analysis.md](product-analysis.md), mainly from section 7. The file [about.md](about.md) describes the product and gives the glossary.

- **Item IDs.** A1, A2 ... are the items of Phase A. B1 ... are the items of Phase B. N1 ... are the items of the Next phase. L1 ... are the items of the Later phase.
- **Status.** Each item has one of these status words: Not started, In progress, Done, Blocked.
- **Effort.** S is 1 to 2 days, M is 3 to 5 days, and L is 6 to 10 days. The effort values are for one maintainer who works with Claude Code.
- **Weeks and months.** Week 1 is the first week of Phase A. Month 3 starts after week 8.
- **Blockers.** The numbers 1 to 16 are the production blockers in [section 4 of the analysis](product-analysis.md#4-what-blocks-production-readiness-today). P0 blockers give a false pass or leak data. P1 blockers stop the public launch. P2 blockers cause false alarms or missed problems that users see in the first week.
- **Ideas and experiments.** Codes such as n01 or e02 are the judged ideas in [section 5 of the analysis](product-analysis.md#5-ranked-opportunities). The audit experiments are e1 to e9 and e3b, and [appendix section B](analysis/appendix.md#b-audit-experiments) describes them. Note: idea codes have 2 digits, for example e02. Experiment codes have 1 digit, for example e2.
- **Test projects.** The 3 test projects are the 3 real projects of the maintainer. The analysis calls them "the 3 real projects".
- **Priorities.** Each item of the Next phase has one of these priorities: "Must, before the public launch", "Must", "Important" or "Optional". Section 7 of the analysis calls the last 2 priorities "Should" and "If time permits".

## Goal

**Position.** Agentproof is the local fact check for AI coding agents. After each turn, Agentproof compares what the agent said with what the agent did.

**Headline.** "Your agent says the tests pass. Agentproof shows if that is true."

**North-star metric: 30-day active installs.** This is the number of installs that ran a check in the last 30 days. Users continue to use a checker only when it finds real problems and gives few false alarms. Thus this one number shows value and trust together.

**Success in 12 months** ([section 3 of the analysis](product-analysis.md#3-positioning)):

- 1,400 installs ran a check in the last 30 days.
- Each rule check that can give a Critical finding has a precision of 90% or more on at least 30 labelled positives.
- Users report fewer than 2 false alarms in 100 sessions.
- Agentproof runs after each turn in Claude Code and Codex, and as a CI step.
- It proves test claims with a proof run on Linux, macOS and WSL2.
- It sends nothing to a server, unless the user turns on the AI check.

## Done: milestones 1 to 4

The maintainer and Claude Code built milestones 1 to 4 on 3 November 2026. The file [project-description.md](project-description.md) has the full progress logs and the test report.

| Milestone | Date | What it delivered | Open items |
|---|---|---|---|
| M1: Skeleton | 3 November 2026 | The engine records the start point, reads a real transcript and finds all changes since the start point, also the commits of the agent. It keeps the changes from before the session separate. The plugin has a manifest, 2 hooks, `/agentproof:check`, a launcher and a local marketplace. The container builds the engine for Linux and Windows. The hooks and `/agentproof:check` work in the Claude desktop app, which shows the message as a "Claude Code notice" | macOS build. PowerShell launcher. The full report inside the desktop app (open question) |
| M2: Rule checks | 3 November 2026 | The engine has ten rule checks (15 rule IDs in the code), each with tests. It gives the verdicts "do not trust this work", "examine N items", "no red flags found" and "incomplete". The message gives the verdict and the top 3 findings, and the JSON file has all findings with evidence. A "bad agent" sample gives "do not trust this work", and a "good agent" sample gives no findings. A check of a real session took 1.5 s and gave 3 findings: recursive deletes of temporary folders. The comprehensive test later counted such deletes as false alarms and fixed the rule (problem 3) | Thresholds in the settings file. Line checks also see the changes from before the session. More safe delete targets |
| M3: Report | 3 November 2026 | The HTML report is one file with 7 sections, light and dark mode, and no external requests. It shows evidence for each finding and escapes all text from the transcript and the repository. The message gives the verdict, up to 3 new findings and the report path. `/agentproof:report` opens the report in the web browser | Test of `/agentproof:report` in the app. Blast radius grouped by intent |
| M4: AI check | 3 November 2026 | The AI check works with Anthropic and with an OpenAI-compatible server, and the tests used a fake server for both. Secret removal runs before the AI check. The AI check runs in the background, and the hook returned in 1.2 s on Windows. The report marks AI findings as opinion. Strict mode stops the agent one time for each new Critical finding. M4 also added the verdict "safe to merge", the user and project settings files and the optional plugin agentproof-ai | Test with a real API key. Thresholds in the settings file. Test of agentproof-ai in the app |
| Comprehensive test | 3 November 2026 | 77 of 77 unit and scenario tests pass. On Windows, 38 of 38 end-to-end hook tests and 27 of 27 end-to-end AI tests pass. The Linux engine and the launcher pass in the container. A check of a 127 MB transcript takes 2.2 s. The test found 11 problems, and commit `28ab57c` fixed them | Old sessions with no start point. `sudo` through `ssh`. No test yet: a real API key, agentproof-ai in a new session, macOS |

### Open items from milestones 1 to 4

| Open item | From | Covered by |
|---|---|---|
| macOS build and macOS test | Milestone 1, test report | B1. The macOS runner is the macOS test |
| PowerShell launcher for Windows with no Git Bash | Milestone 1 | B2 |
| Thresholds (large deletion, repeated error) in the settings file | Milestones 2 and 4 | Not scheduled |
| Test of `/agentproof:report` in the desktop app | Milestone 3 | Not scheduled |
| Blast radius grouped by intent | Milestone 3 | Not scheduled |
| Test with a real API key | Milestone 4, test report | N1 |
| Test of agentproof-ai in a new session | Milestone 4, test report | Not scheduled. Today agentproof-ai is a separate, optional install (see the B4 note) |
| Old sessions with no start point give more false alarms | Test report | Not scheduled |
| `sudo` through `ssh`: the message does not say that the server is remote | Test report | Not scheduled |
| Line checks also see the changes from before the session in the same file | Milestone 2 | Not scheduled |
| More safe delete targets | Milestone 2 | Not scheduled |
| The full report inside the desktop app | Milestone 1 (open question) | Not scheduled |

## Why the plan changed on 4 November 2026

- The first roadmap in [project-description.md](project-description.md) planned 4 milestones, 1 week of daily use and a launch at the end of week 5. The maintainer built milestones 1 to 4 in one day.
- The audit of 4 November 2026 ran 10 experiments on the built engine. It found 8 P0 blockers that give false passes or leak data. For example, a piped test run with failures gives "no red flags found".
- In the sessions of the maintainer, 495 of 525 test commands (94%) piped their output. Thus the exit code was not the test result.
- Users cannot install Agentproof from a release today. The repository has no built engine, so each user must build it in the Docker container. There is no macOS build, and Windows needs Git Bash.
- Other tools already do parts of the first plan. For example, WST2002/proof-of-done finds piped, stale and zero-test runs and the common forms of test tampering.

Thus the new plan fixes the P0 blockers first and moves the public launch to month 4.

## Rules for all work

- **Time limits.** The principle "never slow the agent down" applies to all new work:
  - The start hook adds less than 300 ms on the largest of the 3 test projects.
  - The hook check takes less than 2 s on the largest real session.
  - Each new feature has a test for its time limits.
  - Note: the test report of 3 November 2026 measured 2.2 s for a check of a 127 MB transcript. On 4 November 2026, the A1 engine checked the largest real session (130 MB) in approximately 1.3 s, with a small test repository. This is the median of 5 runs, and the old engine took 1.2 s. A large repository adds time for the `git` commands.
  - On Windows, the first run of a new engine build took 13 s, probably because of a security scan.
- **Regression tests.** Each P0 fix in Phase A gets a regression test (section 7). Each contribution is a fixture and a test (section 9).
- **Late phases.** If a phase is late, move the soft launch. Do not remove tests to save time.
- **Size of a phase.** Each phase has approximately 4 weeks of work.
- **Text.** Write all text for users in ASD-STE100: the message, the report, errors, command help, the README and the documentation.
- **Releases.** Put one finished feature in each release.

## Phase A: weeks 1 to 4. Make the core check true

Phase A fixes only the P0 blockers, each with a regression test. Nobody else uses Agentproof before all Phase A items are Done.

| ID | Work | Blockers | Effort | Done when | Status |
|---|---|---|---|---|---|
| A1 | **Read test output** (n01 core, weeks 1 and 2). Keep the first 2 KB and the last 8 KB of the output of each test command, also in subagents. Mark piped runs "exit code hidden". Parse the summaries of pytest, jest, vitest, cargo and go. Add the test-command forms that the engine does not know: `npm run test:*`, `bun run test`, `python manage.py test`, `nx test` and others. Treat zero-test runs as "not run". Do not trust exit code 0 from a background command until its summary appears | 1, 8 | L | Experiments e1 and e7 give "do not trust this work". A "tests pass" claim after `npm run test:unit` gives no finding. No report says "the last test run passed" for a piped run. The hook check takes less than 2 s on the largest real session | Done, 4 November 2026, commit `f95d880` |
| A2 | **Scope of runs** (n01, week 2). A later run that passes clears a failure only when it ran the same tests or more tests | 2 | S | Experiment e2 gives "do not trust this work" | Done, 4 November 2026, commit `3dd5c19` |
| A3 | **Honest verdict** (n04, verdict part only, weeks 2 and 3). Add the health gate. Hide "safe to merge". Send the end of the output to the AI check. Check the evidence of each AI opinion | 3, 4 | M | Experiments e3 and e3b give "incomplete". The engine never gives "safe to merge" from the AI check alone | Done, 5 November 2026, commit `947d40e` |
| A4 | **Secrets** (week 3). Remove secrets from each saved field. Add key patterns. Fix the order of the secret-file check | 5 | S | Experiment e6 finds none of its fake keys in the JSON file, the report or the AI request | Done, 5 November 2026, commit `9947e13` |
| A5 | **Attribution** (week 3). Do not lower Critical, High, CI or secret findings to Low as "other origin". Treat commands on folders as edits to the files in them | 6 | S | Experiment e4 gives "do not trust this work" | Done, 5 November 2026, commit `6d8f738` |
| A6 | **Ignored data** (week 4). At session start, list ignored and untracked paths with `git ls-files --others --directory`, so that each ignored folder gives one line. Stop the list at 300 ms or at 50,000 entries. Mark a stopped list "partial". Remove the cleanup exception for old data | 7 | M | Experiment e9 gives a Critical finding for the deleted database. The start hook adds less than 300 ms on the largest of the 3 test projects | Done, 5 November 2026. Not committed yet |
| A7 | **Regression suite** (week 4). Add each audit experiment (e1 to e9, e3b) as a fixture test | All blockers with an experiment | S | Each experiment runs as a test, and all tests pass | Not started |

### A1 result (4 November 2026)

A1 is done. The change is in commit `f95d880`.

**What changed:**

- The new file `engine/src/testruns.rs` finds test commands and reads the test output. It knows more forms, for example `npm run test:unit`, `bun run test`, `python manage.py test`, `nx test`, `turbo run test`, `just test`, `bazel test` and `cargo t`. It also finds a runner inside `docker compose run` and `sh -c`.
- The engine reads the summary lines of jest, vitest, pytest, cargo, go, mocha, bun, `node --test`, unittest, rspec, phpunit, dotnet, deno, Maven, ctest and Playwright. Lines of one failed test also count, for example `E   AssertionError` or `FAILED tests/x.py::test_y`.
- The exit code counts only when the shell cannot hide it. A run that found no tests is not a pass. A run in the background has no result until the agent reads its output, or until a notice gives a failure exit code.
- `transcript.rs` keeps the first 2 KB and the last 8 KB of the output of each test command. It adds the commands of subagents, with their results, in time order. Notices about background tasks no longer count as prompts.
- A new finding, `pass_claim_unverified` (High), shows a "tests pass" claim when no test run passed.
- All rule checks for commands also examine the commands of subagents.
- The pass-claim detector now ignores sentences such as "The tests haven't passed" and words such as "passport".
- The report and the AI request say where each test result comes from: the summary line or the exit code.
- A command that does not name a runner is a test run when its output shows a summary line with counts. Examples are `bash run-tests.sh` and a container with tests as its command. File viewers such as `cat` and `grep` are not test runs.

**Code review:** after the change, 4 reviewers examined it, and a skeptic tried to disprove each problem that they found. They found 21 problems, and they could show each of the 12 checked problems with the engine. All of these problems are fixed, each with a test, except one (see the limits below). Examples:

- Commands that only build or list the tests, for example `cargo test --no-run`, counted as passed runs.
- `npm run build -- --mode test` counted as the test script.
- An apostrophe inside double quotes, for example `-t "user's profile"`, hid the later pipe.
- The real notice "failed with exit code 1" and notices in attachment entries were not read.
- The Read tool puts a line number before each line, so the engine did not find the summary.
- A call that the user rejected counted as a failed test run.
- `cargo test && cargo clippy` with a lint error counted as a test failure.
- The sentence "all tests pass, but I haven't committed the change" was not a claim.

**What the tests show:**

- All 101 unit and scenario tests pass, and 25 of them are new. The script `tests/e2e/hooks.sh` passes 38 of 38, and `tests/e2e/ai.sh` passes 27 of 27.
- Experiments e1, e2 and e7 now give "do not trust this work". Experiment e4 also gets a High finding: the agent said "All tests pass", but the last run found no tests.
- A replay of the 26 local sessions of the maintainer read 866 test runs. Of these runs, 506 passed, 235 failed, 13 found no tests, and 112 have no visible result. No verdict changed, and no new Critical finding came from test runs.
- Two sessions got a true High finding for a failed test run at the end. For example, a Playwright class test shows "1 failed", and no later Playwright run passed. Two sessions got Low findings for repeated errors in subagents.
- One session lost a High finding that was not correct. The agent ran the tests with a script, and the new engine reads the summary "334 passed" in the script output.
- Earlier versions of the change gave false Critical findings in 2 sessions, because a script or a Docker service ran the tests. The rule for test runs that the command does not show fixed this.
- The largest session (130 MB) took 1.3 s with the new engine and 1.2 s with the old engine. These are the medians of 5 runs with a small test repository. The slowest run of the new engine took 1.7 s.

**Limits that stay:**

- A later narrower run with the same test runner still clears a failure. A2 fixed this (see "A2 result").
- A later pass clears a failure only when it has the same test runner. Thus `npx jest` and `npm test` are different runners, also when `npm test` starts jest.
- A test run inside a script file counts only when its output shows a summary line with counts.
- The engine does not read the transcripts of workflow subagents (`subagents/workflows`). One session has more than 600 MB of these files, so a check would take too long. See "Not scheduled".
- When the shell hides the exit code and the output has no summary line or failure line, the result is "not known".
- The other P0 blockers stay open. For example, experiments e3, e3b, e6 and e9 give the same results as before.

### A2 result (4 November 2026)

A2 is done. The change is in commit `3dd5c19`.

**What changed:**

- The engine reads the filters of each test command: paths, packages, targets, test names and options. Examples are `tests/test_a.py`, `-p engine`, `--lib`, `-k login` and `pnpm --filter web`. It knows the options of more than 30 runners. Examples are pytest, unittest, Django, jest, vitest, Playwright, cargo, nextest, go, make, Maven, Gradle, rspec and bazel.
- A later run clears a failure only when it ran the same tests or more tests. Its paths, packages and names must hold the earlier tests. Each option of the later run that leaves out tests, for example `-k` or `--skip`, must also be in the earlier run. The settings and the mode must be the same, for example `--config` or `cargo test -- --ignored`.
- A module holds the modules in it, for example `blog` holds `blog.tests`. A file holds its tests and its lines, for example `a.py::test_x` and `a_spec.rb:10`. A shell pattern holds the files that it matches, for example `tests/test_game*.py`. A cargo name holds longer names, for example `parse` holds `parse_empty_input`. A plain name pattern holds a longer pattern, for example `-k login` holds `-k test_login_ok`.
- The engine reads the folder of a run from `cd` and from options such as `npm --prefix` or `make -C`. It compares the folders of two runs only when both commands show them.
- When the output shows left-out tests, for example "2 deselected", but the command shows no filter, the earlier output must show the same. A project setting such as `-m "not slow"` in `pytest.ini` gives this result.
- A run that only its output shows gets the test paths that a script gets, for example `tests/test_a.py` in `./scripts/test.sh tests/test_a.py`.
- A new finding, `narrow_pass_after_failure` (High): a test run failed, and later only a run of a part of the same tests passed. When the final message says that all tests pass, the finding is `tests_failed_but_claimed_pass` (Critical).
- A claim is about all tests only in two cases. The part of the sentence with the claim has the word "suite". Or a word such as "all" comes in the 4 words before the word for tests.
- More commands only show text, so they are not test runs: PowerShell commands such as `Select-Object` and `ForEach-Object`, and `gh run view`. The replay found these A1 problems.

**Code review:** a review with 10 agents examined the first version of A2. They found 8 problems, and they could show each problem with the engine. All 8 problems are fixed, each with a test. Examples:

- A later run that added `-k`, `-t`, `-run` or a cargo test name to the same path cleared the failure of the whole path.
- A later run without the name filter did not clear the failure of the run with the filter.
- `pytest -v tests/test_a.py` counted as a run of all tests, because `-v` counted as an option with a value.
- The `)` of `sh -c "..."`, the file after `&>`, the word `discover` and values such as `--trace on` counted as filters.
- `npm test src/a.test.js` counted as a run of all tests.
- With a project setting that leaves out tests, an equal later run never cleared a failure.
- A pass of other tests changed a false claim from Critical to High.
- The check for a claim about all tests read words outside the claim.

**What the tests show:**

- All 115 unit and scenario tests pass, and 14 of them are new. The script `tests/e2e/hooks.sh` passes 38 of 38, and `tests/e2e/ai.sh` passes 27 of 27.
- Experiment e2 now gives "do not trust this work". The results of the other experiments are the same as after A1.
- A replay of the 26 local sessions of the maintainer read 873 test runs. Of these runs, 512 passed, 230 failed, 13 found no tests, and 118 have no visible result. No verdict changed, and no new Critical finding came from test runs.
- 4 sessions got the new High finding `narrow_pass_after_failure`. Each of these 4 findings is true: a full run failed, and after it only runs of a part of the tests passed. These 4 sessions had the verdict "do not trust this work" before, because of other findings.
- The replay found 3 problems in earlier versions of A2, and they are fixed. First, the folder in the transcript can be older than the folder of the shell after `cd`. Thus the engine does not use the folder in the transcript. Second, the engine read a test file that `sed` changed as a filter of a script run. Third, PowerShell commands that read old logs counted as test runs.
- Speed on the largest session (130 MB): the median of 11 runs is 2.49 s for A2 and 2.35 s for A1. The computer was slower than in the A1 measurement (1.3 s). Thus the 2-second target of A1 needs a new measurement on a quiet computer.

**Limits that stay:**

- A later pass clears a failure only when it has the same test runner. Thus `npx jest` and `npm test` are different runners, also when `npm test` starts jest.
- An option that the engine does not know does not count as a filter. For pytest, a path filter must look like a path, so `pytest unit` counts as a run of all tests. A filter inside a script is not visible, for example in `package.json`.
- The engine compares the folders of two runs only when both commands show them. A run in a folder that holds the folder of a failed run can clear the failure.
- The engine compares name patterns with special characters, and expressions, as text.
- The other P0 blockers stay open, for example in experiments e3, e3b, e6 and e9.

### A3 result (5 November 2026)

A3 is done. The change is in commit `947d40e`.

**What changed:**

- A health check examines the transcript. It gives "incomplete" with a reason in 4 cases. The transcript is empty. No line has a known format. More lines are not valid JSON than lines in a known format. Or files changed, but the transcript shows no edit and no command of the agent.
- The message and the report give the reason of an incomplete check, for example "The transcript is empty."
- The engine no longer gives "safe to merge". The best verdict is "no red flags found", also when the AI check supports each claim. Then the message says that the AI check found evidence for each claim, and that this is an opinion, not a proof.
- The AI request has the last 1,200 characters of the output of the last 10 test commands. For other commands with exit code 0, the label is "exit code 0, the output is not shown", not "passed".
- The engine examines the evidence that the model names. A transcript line must be the line of a step of the session. A file must be a changed file, and a line number must be a changed line or a line of the file. The report marks evidence that the engine did not find.
- A contradicted claim with evidence that the engine did not find gives a Medium finding, not a High finding.

**What the tests show:**

- All 119 unit and scenario tests pass, and 4 of them are new. The script `tests/e2e/hooks.sh` passes 38 of 38, and `tests/e2e/ai.sh` passes 27 of 27.
- Experiments e3 and e3b now give "incomplete". The other experiments give the same results as after A2.
- A replay of the 26 local sessions gave no verdict change. No real session became "incomplete".
- The largest session (130 MB) took 1.5 s in the replay.

**Limits that stay:**

- The replay uses a small test repository, so no file changed in it. Thus the replay did not test the case "files changed, but no edit and no command".
- Nobody tested the evidence check with a real model. The tests use a fake server.
- The engine checks that the evidence exists, but not that the evidence shows the claim.
- A3 does not show the last change and the test result for each changed file. Section 5 of the analysis names this step, but section 7 does not put it in Phase A.

### A4 result (5 November 2026)

A4 is done. The change is in commit `9947e13`.

**What changed:**

- Each text of each saved file goes through secret removal: `check.json`, `last.json`, `ai.json` and `start.json`. The JSON output, the message and the report also go through it.
- The engine finds more key formats: Stripe keys and webhook secrets, npm tokens, GitLab tokens, Slack `xoxe-` tokens and the token after `Bearer`.
- The secret-file check no longer depends on the order of the areas. A file such as `src/auth/secrets.json` is now a secret file, not an authentication file. Thus its content does not go into the report or the AI request.
- For a secret file, the saved changes have only the path.

**What the tests show:**

- All 124 unit and scenario tests pass, and 5 of them are new. The script `tests/e2e/hooks.sh` passes 38 of 38, and `tests/e2e/ai.sh` passes 27 of 27.
- Experiment e6: no fake key is in the JSON output, `check.json`, `last.json` or `report.html`. With the engine before A1, all four had the keys. A unit test shows that the AI request has no keys.
- The other experiments give the same results as after A3.
- A replay of the 26 local sessions with the A3 build and the A4 build gave the same verdicts and findings for each session.
- The largest session (130 MB) took 1.5 s to 2.0 s with both builds. The machine was busy, so the times change from run to run. A4 is not measurably slower.

**Limits that stay:**

- The engine finds only known key formats and values after known names. It does not find unknown key formats.
- The AI check through the agent hook removes no secrets. The checker reads the files and the transcript itself.
- The transcript file of Claude Code keeps all secrets.
- A faster secret check (one pre-check for all patterns) is not in A4. Windows Smart App Control blocked the build with this change, so nobody tested it on Windows.

### A5 result (5 November 2026)

A5 is done. The change is in commit `6d8f738`.

**What changed:**

- Sometimes the transcript does not show that the agent changed a file. A Critical or High finding on such a file keeps its severity. A CI or secret finding also keeps its severity. Each of these findings gets a note. Only a Medium finding outside CI and secrets becomes Low.
- A command that names a folder counts as a change to each file in that folder, for example `git rm -r tests`.
- An MCP tool with a word such as "write", "edit" or "replace" in its name counts as an edit of the file that it names. An MCP tool that only reads does not count.
- New rule `pass_claim_after_tests_removed` (Critical). The agent deleted, removed, skipped or focused tests. Then it said that the tests pass, but no test run passed. Without this rule, experiment e4 gives only "check this work", so the "done when" text needs it.
- The note on these findings is now clear: "The transcript does not show that the agent changed this file."

**What the tests show:**

- All 128 unit and scenario tests pass, and 4 of them are new. The script `tests/e2e/hooks.sh` passes 38 of 38, and `tests/e2e/ai.sh` passes 28 of 28.
- In `tests/e2e/ai.sh`, the changed `.env` file is now Medium, so it pushes the AI finding out of the short message. The test now finds the AI finding in the saved result. It has 1 more assertion.
- Experiment e4 gives "do not trust this work". Experiment e6 now gives "check this work", because its changed secret file stays Medium. Experiment e3 still gives "incomplete", but its deleted test file is now High. The other experiments give the same results as after A4.
- A replay of the 26 local sessions with the A4 build and the A5 build gave the same verdicts and findings for each session.

**Limits that stay:**

- The replay uses a small test repository, so no file changed in it. Thus the replay did not test the new attribution on real sessions.
- The words in MCP tool names are a guess. An MCP tool with an unusual name is still not counted.
- A command with a broad word such as `install` still counts all files as changed.
- The new Critical rule needs a pass claim with no passing run. An agent that deletes a failing test and then runs the other tests with a pass is not Critical. Then the deleted test file is High.

### A6 result (5 November 2026)

A6 is done. It is not committed yet.

**What changed:**

- At the session start, the engine runs `git ls-files --others --directory`. The start point keeps the list of paths that Git does not track, ignored paths too. A folder with no tracked file is one line, for example `data/` or `node_modules/`.
- The list stops after 300 ms or 50,000 paths. Then the start point marks it "partial".
- A recursive delete counts as cleanup only when the engine has the full list, and the folder was not in it. A folder inside an untracked folder of the start point also existed. A partial list, or a start point of an older version, gives no cleanup exception.
- `check.json` does not keep the list. Only `start.json` keeps it.
- A check with no recorded start point does not make the list, because it cannot show the start.

**What the tests show:**

- All 131 unit and scenario tests pass, and 3 of them are new. The script `tests/e2e/hooks.sh` passes 38 of 38, and `tests/e2e/ai.sh` passes 28 of 28.
- I made experiment e9 again with a new start point. The delete of the ignored `data` folder gives a Critical finding and "do not trust this work". The engine before A6 gave "no red flags found". The saved e9 start point of the older version also gives Critical now, because it has no list.
- The other experiments give the same results as after A5.
- A replay of the 26 local sessions with the A5 build and the A6 build gave the same verdicts and findings for each session.
- The start hook ran 7 times on each project of the maintainer. A6 adds less than 300 ms on each project. These are the median times:
  - 4,992 tracked files: 512 ms with A5, 678 ms with A6.
  - 1,251 tracked files: 350 ms with A5, 428 ms with A6.
  - 557 tracked files: 536 ms with A5, 783 ms with A6.
  - 51 tracked files: 456 ms with A5, 516 ms with A6.

**Limits that stay:**

- On the largest project, each Git command takes 0.35 to 0.46 s on Windows. The list was partial in 1 of 7 runs. In an earlier run on a busy machine, it was partial in each run. Then no recursive delete counts as cleanup, so a delete of a session folder can give a false alarm.
- A better version starts the list before the other start work, so that both run at the same time. Windows Smart App Control blocked that build, so A6 does not include it.
- The replay sessions have no start point, so the replay did not test the cleanup exception.
- A session that started before this version has a start point with no list. For it, each recursive delete of a folder that the session made gives a finding.

### How to do A1 and A2

These details come from [section 5, item 1](product-analysis.md#the-top-10) of the analysis (the idea "Test forensics", n01).

- **Test command.** A test command is a Bash or PowerShell call in which one pipeline stage starts with a known test runner. This also applies inside `docker compose run` or `sh -c`. Note: section 5 names only Bash, but the engine also reads PowerShell commands today.
- **Piped run.** A piped run sends the output of the runner into `|`, and the command has no `pipefail`. Mark a run "exit code hidden" when it pipes into `tail`, `head` or `grep` without `pipefail`.
- **Summaries.** Parse the summary lines of pytest, jest, vitest, cargo and go first. Treat "0 tests", "no tests ran" and "collected 0 items" as "not run".
- **Background commands.** Do not trust exit code 0 from a background command until its summary line appears (Claude Code issue #90659).
- **Scope (A2).** A later run that passes clears a failure only when it ran the same tests or more tests.
- **Main risk.** Honest partial claims look like false claims, for example "the new lock test passes". Set the severity from the scope of the claim.
- Warning: strict mode must not stop the agent for these findings until the history audit (B7) shows almost no false Critical findings.
- **Evidence.** The sample was 15 sessions in 4 projects, from 21 October to 3 November 2026. In this sample, 495 of 525 test commands (94%) piped their output, mostly into `grep` (271) and `tail` (218).

### How to do A3

These details come from [section 5, item 2](product-analysis.md#the-top-10) of the analysis (honest verdicts, n04).

- The health gate gives "incomplete" with a reason.
- "Safe to merge" stays hidden until a proof from a proof run exists. The proof run (N12 and L1) can give this proof later.
- An AI opinion on a claim counts only when it cites a real line of the transcript or of the diff.
- Section 5 also says: for each changed file, show the last change and the test result after that change. Note: section 7 does not put this step in Phase A, so the maintainer decides if A3 includes it.
- Main risk: the best verdict becomes "no red flags found". Users can think that this verdict is weak, but it is the honest result.
- [Appendix section A4](analysis/appendix.md#a4-quick-wins) gives more details for A1 to A6, for example the conditions for the health gate.

### The code today

A code review on 4 November 2026 found these facts. Read them when an item starts.

| ID | Code today |
|---|---|
| A1 | Done. See "A1 result" above |
| A2 | Done. See "A2 result" above |
| A3 | Done. See "A3 result" above |
| A4 | Done. See "A4 result" above |
| A5 | Done. See "A5 result" above |
| A6 | Done. See "A6 result" above |
| A7 | `engine/tests/data/` has 3 sample transcripts. The end-to-end scripts run only on Windows. The experiment files are not in the repository. Make them again from appendix section B |

Note: experiments e5 and e8 show blockers 14 and 16. N2 and N3 fix these blockers in the Next phase. Section 7 does not say what these 2 tests expect at the end of Phase A.

## Phase B: weeks 5 to 8. Make the install work, then a soft launch

Phase B makes the install work, adds the license and CI, measures the precision and ends with a soft launch. It fixes the P1 blockers that the soft launch needs.

| ID | Work | Blockers | Effort | Done when | Status |
|---|---|---|---|---|---|
| B1 | **Binaries** (weeks 5 and 6). Use cargo-dist for Windows, Linux and macOS (x64 and arm64), with checksums. Put the binaries in the plugin release, so that no download occurs at run time. Document the macOS quarantine step and the Windows SmartScreen warning until the binaries have signatures (N11). Make shell, PowerShell, Homebrew tap and MSI installers with cargo-dist (section 9) | 9 | M | A GitHub Actions macOS runner and a Windows runner install from the release and give a verdict on a fixture session. The maintainer has no Mac, so the macOS runner is the test | Not started |
| B2 | **Windows launcher** (week 5). Add a PowerShell launcher next to the Bash launcher. Claude Code runs hook commands in Git Bash on Windows, and in PowerShell when the computer has no Git Bash. Make the hook command start the correct launcher in each shell. As an alternative, use the exec form, which starts a real `.exe` file with no shell | 9 | S | The end-to-end script passes on a Windows runner with no Git Bash on `PATH` | Not started |
| B3 | **License, CI, security policy** (week 6). Add `LICENSE-MIT`, `LICENSE-APACHE` and `SECURITY.md`. Run `cargo test`, `clippy` and the end-to-end scripts in CI | 10 | S | CI passes on 3 systems for each pull request | Not started |
| B4 | **Safe AI defaults** (week 6). Never wait for the AI check in a hook, also in strict mode. Do not put agentproof-ai in the default install. Remove the false README claim about the next message | 11, 12 | S | No hook waits for an AI call. The README has no false AI claim | Not started |
| B5 | **Data on disk** (week 7). Add `agentproof clean --older-than 30d`, a 30-day default limit and a setting that removes the full session from the JSON file. Add the "Data on disk" paragraph to the README | 15 | S | With the default limit, the data folder has no session folder that is older than 30 days. With the setting on, the JSON file has no session text | Not started |
| B6 | **First run and feedback** (week 7). Add `/agentproof:doctor` (an install test) and `/agentproof:wrong` (a false-alarm report). Add issue templates for false alarms and missed failures | None | S | On a clean runner, the first message confirms that both hooks ran. `/agentproof:wrong` writes a redacted local file | Not started |
| B7 | **History audit, first version (n11), and the labelled set** (weeks 7 and 8). Replay the transcripts of the maintainer. Label a sample by hand. Add synthetic fixtures for the rule checks that rarely give a finding in real sessions | None | L | Each rule check that can give a Critical finding has at least 30 labelled positives, or it has the mark "unmeasured". The README shows the precision of each rule check, with separate real and synthetic results | Not started |
| B8 | **Precision gate** (week 8). A rule check with low or unmeasured precision cannot give a Critical finding and cannot stop the agent | None | S | A rule check with a measured precision under 90% cannot give a Critical finding or stop the agent in strict mode. This is also true for an unmeasured rule check | Not started |
| B9 | **Name and README** (week 8). Check the name on GitHub, npm, PyPI, plugin marketplaces, domains and trademarks. Rewrite the README with the new product position and no false claims | None | S | No test and no audit experiment shows a false claim in the README | Not started |
| B10 | **Soft launch** (end of week 8). Make the repository public. Submit Agentproof to the Claude plugin directory and to awesome-claude-code. Use Agentproof on its own development each day | None | S | The maintainer submitted Agentproof to the Claude plugin directory and saved 5 verified findings as redacted screenshots | Not started |

### More details for Phase B

- **B3** (section 11). Keep sample transcripts from each Claude Code version. Run them in CI.
- **B4.** Note: today agentproof-ai is not in the default install. It is a separate plugin in the marketplace file, and the README gives it as an optional install step. Sections 4 and 7 say "remove it from the default install". They do not say if B4 removes the marketplace entry or only changes the README.
- **B6** ([section 9](product-analysis.md#9-community-and-growth-plan)):
  - The install text tells the user to start a new session, because Claude Code loads a plugin only at session start.
  - The first message confirms that both hooks ran. It gives the report path and the state of 3 settings: AI check off, strict mode off, proof run not available yet.
  - `/agentproof:doctor` shows the engine version, the launcher and the shell. It confirms that the start hook ran in this session and that the engine can read the transcript. If the start hook did not run, it tells the user to start a new session.
  - After Phase B, the first message suggests `/agentproof:audit` for the last 30 days. The user can attach the file from `/agentproof:wrong` to the "false alarm" issue template.
  - The documentation explains that strict mode stops the agent one time for each new proven Critical finding.
- **B7** (section 5, items 5 and 6):
  - `agentproof audit --days 30` replays local transcripts with the rule checks that need no start point. It prints counts with labels such as "checked from output" or "exit code only", with links to the sessions.
  - Add the command `/agentproof:audit`, which runs `agentproof audit --days 30`.
  - It also shows the count of the items that it could not check. It shows the sessions with Superpowers verification-before-completion in a separate list, so that users see what that skill misses.
  - Add facts from Git for each commit. For example, show how many agent commits had no test run that passed after the last edit.
  - Make the labelled set from real sessions of the maintainer, synthetic "bad agent" sessions and ImpossibleBench-style cases (MIT license). Report the real results and the synthetic results in separate lists.
  - A rule check with fewer than 30 positives is "unmeasured" and cannot give a Critical finding. Publish precision and recall for each rule check in the README, with the sample size.
  - Run the set in CI for each release. Keep the set small and versioned. State the limits of the set. Plan the maintenance of the set. Do not score competitors at first.
- **B9** (sections 3, 4 and 10):
  - Do not use the claims in the list "Stop claiming these" in section 3. Examples are "Nobody else reads the transcript", "Transcript plus git is unique" and "Safe to merge".
  - Keep a claim about AI findings or secret removal only when a test shows that the claim is true.
  - Note: section 4 calls "the report removes secrets" a README claim. The README does not have this claim. The test report in [project-description.md](project-description.md) (problem 9) and [about.md](about.md) have similar statements.
  - Do not make the rule check for dangerous commands the main feature.
  - Say in the README that the core stays free (MIT OR Apache-2.0), as section 10 says.
- **B10** (section 9):
  - Submit Agentproof at claude.ai/directory/manage. In the data questions, say that data leaves the computer only when the user turns on the AI check.
  - Publish `claude plugin install agentproof --marketplace <owner>/<repo>` for the self-hosted marketplace (Claude Code 2.1.292 or later). Tell users to turn on auto-update, which is off by default.
  - For awesome-claude-code, follow its `CONTRIBUTING.md` process.

## Soft launch gate (end of week 8)

Make the repository public only when all these conditions are true (sections 7, 8 and 9 of the analysis):

- [ ] All items of Phase A have the status Done. Nobody else uses Agentproof before this.
- [ ] Items B1 to B9 have the status Done.
- [ ] The release has binaries for Windows, Linux and macOS, with checksums. No download occurs at run time (B1).
- [ ] One command adds the marketplace, and one command installs the plugin.
- [ ] The README gives the requirements: Git, and Git Bash on Windows until a release has the PowerShell launcher (B2).
- [ ] The repository has `LICENSE-MIT`, `LICENSE-APACHE` and `SECURITY.md`, and CI passes on 3 systems (B3).
- [ ] No hook waits for an AI call, and agentproof-ai is not in the default install (B4).
- [ ] The README has no false claim (B4, B9). Each claim about AI findings or secret removal is true, or the README does not have it.
- [ ] The README has the "Data on disk" paragraph (B5).
- [ ] The README shows the precision of each rule check, with separate real and synthetic results (B7).
- [ ] No unmeasured rule check, and no rule check under 90% precision, can give a Critical finding or stop the agent (B8).
- [ ] The name check is complete (B9).
- [ ] The maintainer saved 5 verified findings as redacted screenshots (B10).
- [ ] Agentproof has no telemetry and needs no account or server.

Note: the 5 screenshots come from the daily use of Agentproof on its own development (B10). Thus start this daily use before the end of week 8. When all conditions are true, make the repository public and submit Agentproof (B10).

## Next: months 3 to 5

The Next phase fixes the other P1 blockers before the public launch in month 4. It also fixes blocker 16 and adds the first new features.

| ID | Work | Blockers | Effort | Done when | Priority | Status |
|---|---|---|---|---|---|---|
| N1 | **AI check repairs and cost rules.** Show the latest finished AI result. Test with real keys. Run the call only on turns that changed files and make a claim. Show the cost of each call | 11, 13 | M | Real-key tests pass for Anthropic and one OpenAI-compatible server. The report shows the latest AI result and its cost | Must, before the public launch | Not started |
| N2 | **False alarms for requested work** (n12, first part). Map the words in the prompts to areas. Lower each finding that matches a word in the prompts to Low. Add the note "You asked for this" and the prompt line | 14 | M | Experiment e5 gives only Low findings, each with the note "You asked for this" and the prompt line | Must, before the public launch | Not started |
| N3 | **Destructive patterns, with the Windows forms.** Add the patterns of section 4, each with a fixture. Scan scripts that the agent wrote and then ran. Read the prompts before you set the severity of a force push | 16 | M | Experiment e8 and the fixture for each new pattern give a finding. A force push that the user asked for gives a Low finding | Must | Not started |
| N4 | **Secret value added in the diff** (new rule check, High). Use gitleaks-style rules and entropy on added lines only. Use the same pattern set as the secret removal, with validation off. Also give a finding for `cat .env` and similar commands that print secrets into the transcript | None | M | Fixtures that contain fake keys give a High finding. The real sessions of the maintainer give no false High finding | Must | Not started |
| N5 | **Test-tampering chain with the strict gates** (n02) | None | M | It finds each tampering fixture and gives no High finding on the real sessions of the maintainer | Must | Not started |
| N6 | **Lost-data finding** (n07, first part) | None | M | Deletion fixtures give a Critical finding. Real sessions give no Critical finding for temporary files | Must | Not started |
| N7 | **A CLI that reads transcript files, and a CI step** (e02 and n21) | None | M | A sample workflow runs Agentproof after claude-code-action and fails the job on a Critical finding, with the reason in the job summary | Must | Not started |
| N8 | **Public launch in month 4:** one post and Hacker News | None | S | The maintainer published the post with verified findings and precision values. Agentproof is available in the Claude plugin directory, or the self-hosted install works | Must | Not started |
| N9 | **Narrow claim checks** (n05) | None | M | Each claim type has a precision of 90% or more on labelled final messages | Must | Not started |
| N10 | **Codex adapter** (e01, Codex only) | None | L | 20 real Codex sessions give verdicts. Unknown lines give "incomplete", never "no red flags found" | Must | Not started |
| N11 | **Sign the binaries:** Apple Developer ID with notarization, and Windows Authenticode | None | S | A binary from the release starts with no warning on clean macOS and Windows runners | Important | Not started |
| N12 | **Proof run, first version, on request** (n03) | None | L | `/agentproof:prove` proves or disproves each new test on the 3 test projects in WSL2 and on a Linux runner. It keeps "could not run" separate from "failed". On native Windows, `/agentproof:prove` shows "not available yet" | Important | Not started |
| N13 | **Accept once** (n12, second part) | None | S | Agentproof does not show an accepted finding again until its evidence changes | Optional | Not started |
| N14 | **Dependency evidence, offline part** (n16) | None | M | Agentproof shows each new package by name, with its install errors and the typo distance to popular package names | Optional | Not started |
| N15 | **Push gate, opt-in** (n08) | None | S | The push gate asks the user one time before a push, a new pull request or a merge. It asks only when a proven Critical finding is open, and it shows the evidence | Optional | Not started |

### More details for the Next items

- **N1. Rules for the AI check** ([section 10](product-analysis.md#cost-of-the-optional-ai-check)):
  - Run the call only when the turn changed files and the final message makes a claim about the work.
  - Show the estimated cost per call and per day in the report and in the settings documentation. After each call, show the real cost from the token counts in the answer.
  - Add a daily cost limit to the user settings. If a call costs more than the rest of the limit, skip the call. Then write the reason in the report.
  - Test `claude-haiku-5-5` and `claude-sonnet-5-5` on the labelled set. Make the cheapest model with the same precision the default. Keep `claude-opus-5-5` as an option.
  - The agentproof-ai plugin costs no money, but it uses plan limits on each turn. Measure that use before you recommend the plugin.
  - Estimated cost today: with `claude-opus-5-5` and a call on each turn, a median day costs approximately $8 to $12. The busiest day costs approximately $29. Nobody measured the real cost, because nobody tested the AI check with a real API key.
- **N3. Destructive patterns** (section 4). Each pattern needs one fixture:
  - Windows: `rmdir /s /q`, `del /s`, and `Remove-Item -Recurse -Force` on a drive root or the home folder.
  - Paths that resolve to a root: an MSYS path of `\` or `/`, `cygpath` output, 8.3 short names, and empty variables such as `rm -rf "$DIR/"`.
  - Git: `git checkout -- .`, `git restore`, `git stash` when the user has changes, and `git branch -D`.
  - Files: `find -delete`.
  - Data and infrastructure: `DROP` or `TRUNCATE`, `prisma migrate reset`, `terraform destroy`, `aws s3 rm --recursive` and `kubectl delete`.
  - Scripts: a script that the agent wrote in the session and then ran (Claude Code issue #88462). Scan its text with the same patterns.
  - Note: the engine already finds `rmdir /s`, `rd /s` and `Remove-Item -Recurse` as the first word of a command part. The engine splits commands into parts at `;`, `&` and pipes. It also finds `rm -rf "$DIR/"`. It does not find `del /s`, or these commands inside `cmd /c` or `powershell -Command`. The forms that the engine finds need only the fixture.
- **N5. Test-tampering chain** (section 5, item 3):
  - Make 2 parts. The first part finds the common forms, as WST2002/proof-of-done does. These forms are skipped or focused tests, weaker assertions, assertions that always pass, deleted tests, hidden errors and lower coverage thresholds. The second part is the copied-value chain. A test fails, the agent changes the expected value to the value from the failure, and the test passes.
  - Take the expected and the received values of the test that failed from the parsed output. Find the edits between that failure and the next run that passes.
  - Give a High finding only when all of these conditions are true:
    - The test existed at the session start.
    - It failed before any edit to source code.
    - Only the test or the test settings changed.
    - The failure was an assertion.
    - The final message did not tell the user about the change.
  - Show the other cases as one "examine" line with the values. In the sessions of the maintainer, 10 of 100 periods from a failed run to the next passed run had edits only in tests. None of these 10 periods had test tampering.
- **N6. Lost-data finding** (section 5, item 4):
  - A6 makes the list of ignored and untracked paths at session start. Add the size and the time of each path.
  - At each stop, find the paths that existed at the start and are now missing or empty. Give a Critical finding only for valuable data: databases, uploads, untracked source files and uncommitted changes.
  - Skip temporary files such as logs, locks and SQLite `-wal` files. Do not add the backup copy and the question to the user before a deletion in N6. Add them later.
- **N7. CLI and CI step** (section 5, item 7):
  - Add `--since <commit>` and `--fail-on <level>` to `agentproof check`. Use the existing `--transcript <file>` option for the transcript file. Note: section 5 writes `--session <file>`, but today `--session` takes a session ID.
  - Set the default level to Critical. Write a versioned JSON file and a GitHub job summary.
  - Exit code 0 means that the check passed. Exit code 2 means findings at or above the level. Exit code 1 means a failed or incomplete check.
  - Publish a versioned JSON schema for the check result (section 9). It holds the verdict, the findings, the evidence and the rule checks that could not run.
  - Publish the CI step in GitHub Marketplace (section 9).
- **N8. Public launch** (section 9):
  - In month 3, publish one technical post. In the post, say which data comes from the maintainer. Also say how small that data set is.
  - In month 4, post on Hacker News when all conditions of the public launch gate are true. Post between 12:00 and 17:00 UTC with a plain title.
  - Answer where people already ask, for example in Claude Code issues #104504 and #103500. Reply only where Agentproof solves the problem. Reply one time only.
  - Ask for reviews from people who wrote about agents that change tests.
- **N9. Narrow claim checks** (section 5, item 8):
  - Keep the end of the output of build, lint and type-check commands, as A1 does for test commands.
  - Check the claims "build OK", "lint OK", "type check OK", "pushed" and "no new dependencies". Check each claim against the last run after the last edit. Put each sentence that the rule checks cannot check in a "not checked" list.
  - Give a finding for a stub such as `todo!()` only when the final message says "done" and names that file. Give "contradicted" only for a direct mismatch. First, measure the precision on the 1,047 final messages in the local sessions of the maintainer.
- **N10. Codex adapter** (section 5, item 9):
  - Add an adapter layer and a coverage record for each rule check. Use only the `SessionStart` and `Stop` hooks. Parse Codex transcript files. Test the parser with real sample files from each Codex version.
  - Mark each rule check that cannot run on Codex as "not checked". Codex exit codes can arrive late.
  - The Codex transcript format is not a stable interface, and it can change. Do not add Cursor in N10, because its shell hook has no documented exit code.
  - Release a Codex plugin or hooks bundle with the adapter in month 5 (section 9).
- **N12. Proof run** (section 5, item 10):
  - At first, run it only on `/agentproof:prove`. Make 2 clean copies outside the repository: the start state and the end state. Run the test command of the agent in both copies.
  - Report only the differences: proven new tests, regressions, and runs that could not run. Store the start content of changed files. When the environment fails, never make the agent start work again.
  - Safety (section 11): the proof run runs code from the repository. Only the user settings file can turn it on. Ask the user for consent for each project. Use a sandbox with no network. Never turn it on by default.
  - Warning: never use the sandbox runtime (`srt`) without `--skip-profile-grant`. On Windows, `srt` is an alpha version, HTTPS with Schannel fails, and its profile grant exposes `.npmrc` tokens.

## Public launch gate (month 4)

Post on Hacker News only when all these conditions are true (sections 7, 8 and 9 of the analysis):

- [ ] Phase A removed the false passes: A1 to A7 have the status Done.
- [ ] The install works: Agentproof is available in the Claude plugin directory, or the self-hosted install works.
- [ ] The README shows the precision of each rule check (B7).
- [ ] A release has the test-tampering chain (N5).
- [ ] The P1 blockers 9 to 15 have fixes: B1 to B5, N1 and N2 have the status Done.
- [ ] The "Must, before the public launch" items N1 and N2 have the status Done.

Note: there is only one main launch. If a condition is not true, move the public launch.

## Later: months 6 to 12

| ID | Work | Done when | Status |
|---|---|---|---|
| L1 | Proof run in the background (`asyncRewake` hook), and proof levels in the verdict | The engine gives "safe to merge" again, but only with a proof from a proof run. The verdict names its proof level | Not started |
| L2 | Pull request proof Action (n20), free for public repositories | 10 public repositories use it | Not started |
| L3 | Cursor (preview) and OpenCode. Readers for Entire checkpoints, Agent Trace and ATIF | Each agent has a coverage table. For each agent, the verdict is never "no red flags found" when an important rule check cannot run | Not started |
| L4 | User limits ("You said don't", n06): a check of the limits from prompts and rule files, as a small addition to the current findings | Zero false alarms on a labelled set of real prompts | Not started |
| L5 | Local history, with counts before and after the user changes a habit | The history audit shows the difference after the user changes a habit | Not started |
| L6 | A team policy file that Agentproof reads only from the default branch (n12, third part) | A policy change inside a pull request has no effect and gives a finding | Not started |
| L7 | Evidence card and repro bundle (n24) | A user can share a redacted card for a Critical or High finding | Not started |
| L8 | More security checks: the path of each secret, with no added fake key (n15). Changes that turn off checks with no request from the user (n14). Research on the origin of risky actions (n13) | Each check has a labelled fixture set and a published precision before its release | Not started |
| L9 | Open dataset | The labelled set is public, and at least one other tool reports results on it | Not started |
| L10 | Business tests: sponsors first, and a paid CI option only if the Action has users | Sponsor income or a first team that pays, with no change to the free core | Not started |

Section 5 of the analysis also gives the priority "Later" to 4 more ideas. They are the trust status line (n10, optional), the editor lens (e06), the ACP tap (e05, possibly) and the AST rule core (n17, if necessary). This plan does not schedule them.

## Not scheduled

These known gaps and plans have no item yet, and they are not urgent. To schedule one, move it to a phase. Then give it the next unused ID in that phase.

| Gap or plan | From | Note |
|---|---|---|
| Users cannot change the thresholds. A large deletion is 200 lines or more, and a repeated error occurs 3 times or more | Milestones 2 and 4, section 4 | The thresholds are constants in `checks.rs` |
| No session history. Each session has only the result of its last check | Section 4 | L5 adds only counts before and after the user changes a habit |
| Old sessions with no start point give more false alarms. A delete of a folder that the session made still counts as dangerous | Section 4, test report | With no start point, the base is the last commit |
| `sudo` through `ssh`: the message does not say that the server is remote | Test report | Note: the rule finds `sudo` only as the first word of a command part. The engine splits commands into parts at `;`, `&` and pipes. Thus `ssh host sudo ...` gives no finding. But `ssh host "cd /app && sudo ..."` gives a Critical finding, and the message does not say that the server is remote |
| Blast radius grouped by intent | Milestone 3 | It needs the AI check. Today the table marks test files, sensitive files and files outside the folders that the prompts name. It also marks files that the agent did not touch, and files that also changed before the session |
| Line checks on a file with changes from before the session also see the earlier changes | Milestone 2 | The start point keeps only a content ID. N12 stores the start content of changed files for the proof run |
| More safe delete targets | Milestone 2 | Note: the code review found that the safe-target rule is already too wide, for example for `rm -rf ~/build` |
| A test of `/agentproof:report` in the desktop app | Milestone 3 | None |
| A test of agentproof-ai in a new session | Milestone 4, test report | Today agentproof-ai is a separate, optional install (see the B4 note). Section 8 says: do not put it in the default install before it passes tests |
| The full report inside the desktop app | Milestone 1 (open question) | Today the app shows only the message |
| Read the transcripts of workflow subagents (`subagents/workflows`) with a cache that reads only the new lines of each file | Code review of A1 | Blocker 8 needs this for full coverage. These files can be larger than 600 MB in one session, so the 2 s time limit needs the cache |
| Native hooks: `ConfigChange` and `InstructionsLoaded` for sensitive paths, `PostToolUseFailure` for repeated errors, and `SubagentStop` for subagent claims | Section 11 | Part of the plan for the risk "Anthropic builds a native claim check" |
| Contributors: good first issues with a label, and a reply in 2 days or less | Section 9 | B6 adds the issue templates. Write the contributor guide in ASD-STE100 |
| A separate crate with the parsers for test output, with the MIT license | Section 9 | None |
| Content: a page of verified findings with redacted evidence, only from users who agree | Section 9 | Update the accuracy numbers with each release |
| Content: one short post for each type of failure, and a 30-second screen recording of one verified finding | Section 9 | None |

## Blocker map

Each blocker of [section 4](product-analysis.md#4-what-blocks-production-readiness-today) has at least one item.

| # | Level | Blocker | Items |
|---|---|---|---|
| 1 | P0 | The main rule check trusts the exit code and does not know common test commands | A1 (done), A7 |
| 2 | P0 | A narrower run that passes clears an earlier full failure | A2 (done), A7 |
| 3 | P0 | "Safe to merge" can come after test failures that the engine did not see | A3 (done), then L1 |
| 4 | P0 | An unknown or empty transcript gives "no red flags found" | A3 (done), A7 |
| 5 | P0 | The JSON file and the report keep secrets | A4 (done), A7 |
| 6 | P0 | Edits through MCP tools or folder commands get the severity Low as "other origin" | A5 (done), A7 |
| 7 | P0 | A delete of old, ignored data counts as "cleanup" | A6 (done), A7 |
| 8 | P0 | Test runs in subagents have no results | A1 (done), A7 |
| 9 | P1 | Nobody can install Agentproof without a manual build | B1, B2 |
| 10 | P1 | No `LICENSE` file, no CI and no `SECURITY.md` | B3 |
| 11 | P1 | The AI result rarely reaches the user. In strict mode, the AI call can stop the agent for up to 5 minutes | B4, N1 |
| 12 | P1 | Nobody tested the agentproof-ai plugin | B4 |
| 13 | P1 | Nobody tested the AI check with a real API key | N1 |
| 14 | P1 | Work that the user asked for gives false alarms | N2 (A7 adds the e5 test) |
| 15 | P1 | Data on disk has no limit | B5 |
| 16 | P2 | The engine does not find many destructive commands. A force push that the user asked for gives "do not trust this work" | N3 (A7 adds the e8 test) |

## What we will not do

These rules come from [section 8 of the analysis](product-analysis.md#8-what-not-to-do). The rule about a price for each review comes from section 10.

- **Do not build an AI diff reviewer** (a CodeRabbit clone). Many tools already do it, and it does not use the transcript, which is the unique data of Agentproof.
- **Do not launch on Hacker News too early.** First, fix the false passes. Then make the install work. Then publish the precision. There is only one main launch.
- **Do not give "safe to merge" from an AI opinion.** A trust tool that says "safe" by mistake loses all trust.
- **Do not put agentproof-ai in the default install** until it passes tests and the report shows its result.
- **Do not add many agents before the Claude Code checks are accurate.** Add only Codex, after the core is correct.
- **Do not build an agent scorecard, an Agent Honesty Index or agent duels.** The events are rare, the task mix is different for each agent, and nobody can check the numbers.
- **Do not make a new open standard now.** Nobody will use a format from a tool with no users. Publish a JSON schema. Read the current formats.
- **Do not add telemetry, an account or a server.** Use the Usage tab of the directory and the download count of GitHub Releases for numbers.
- **Do not fix code automatically or write to the repository.** This breaks two core principles.
- **Do not stop the agent for an AI opinion or for rule checks with low precision.** Stop the agent only for proven Critical findings, and only one time.
- **Do not make the rule check for dangerous commands the main feature.** Native features and guards stop most dangerous commands earlier. Mention this rule check for bypass mode, headless runs, CI and agents other than Claude Code.
- **Do not ask the user before each command.** One opt-in gate asks only before a push, a new pull request or a merge while a proven Critical finding is open. Auto mode misses many risky actions. Thus Agentproof checks the work after each turn.
- **Do not run the AI check on each turn by default.** Run it only on turns that changed files and make a claim. Show its cost.
- **Do not make rule checks available only with a paid license later.** Semgrep did this, and the community made a fork (Opengrep).
- **Do not charge a price for each review.** Users do not like this model (sections 6 and 10).
- **Do not bundle code with restrictive licenses.** Examples are Semgrep registry rules, CodeQL and TruffleHog (AGPL). Do not copy code from ablanchard-dev/claimcheck (GPL) or Vet (AGPL).

## Targets

Section 9 of the analysis gives these values as the first targets. Change them after the first month of data.

| Metric | Source | Month 3 | Month 6 | Month 12 |
|---|---|---|---|---|
| **30-day active installs (north star)** | Usage tab of the directory | 60 | 350 | 1,400 |
| Installs | Usage tab of the directory | 200 | 1,200 | 4,000 |
| Downloads of GitHub Releases (the measure before Agentproof is available in the directory) | GitHub API | 300 | 1,000 | 3,000 |
| Precision of each rule check that can give a Critical finding | Labelled set: at least 30 positives for each rule check, with separate real and synthetic results | 90% or more | 90% or more | 95% or more |
| Verified findings in 100 sessions | History audit of the sessions of the maintainer, and history audits that users share | Publish the first value | Publish with each release | Publish with each release |
| False-alarm reports in 100 sessions | `/agentproof:wrong` reports and issues, divided by the runs of the start hook | Measure | Under 5 | Under 2 |
| Verified findings that users share | Issues and posts | 5 | 20 | 50 |
| Supported agents | Releases | Claude Code | Claude Code, Codex and the CI step | Claude Code, Codex, the CI step and one more agent |
| Outside contributors | GitHub | 3 | 10 | 20 |

Note: section 3 of the analysis gives 90% or more as the precision target for month 12. This table gives 95% or more.

**How the maintainer measures with no telemetry:**

- The Usage tab of the Claude plugin directory shows installs, retention and hook runs.
- Installs from the self-hosted marketplace give no Usage tab data. Thus, before Agentproof is available in the directory, count the downloads of GitHub Releases. Also use them if Agentproof is not available in the directory by month 3.
- For false alarms, count the `/agentproof:wrong` reports and the issues. The reports show fewer false alarms than the real number. Thus use the reports only to see trends.
- A verified finding is a High or Critical finding that a person confirmed as true.
- Stars are only a second measure. The first design document set these other targets:
  - Launch week: the front page of Hacker News or 500+ stars, and 5 verified findings as screenshots.
  - 3 months: 1,000+ stars, 10+ outside contributors and a second agent.
  - 6 months: people that the maintainer does not know use Agentproof each week. At least two newsletters or YouTube channels show it. Agentproof checks its own pull requests.

## Decisions

### Decided

| Date | Decision |
|---|---|
| 3 November 2026 | The name is Agentproof. The name is available on crates.io. The check of GitHub, npm, PyPI, plugin marketplaces, domains and trademarks is not complete (B9) |
| 3 November 2026 | Development occurs in a local Docker container |
| 3 November 2026 | Agentproof is a Claude Code plugin first, not a standalone command |
| 3 November 2026 | All text for users follows ASD-STE100: the report, the message, errors, command help, the README and the documentation |
| 3 November 2026 (the date of the design document) | Without the AI check, the best verdict is "no red flags found", never "safe". The analysis of 4 November 2026 adds: "safe to merge" must come only after a proof from a proof run (A3, L1) |
| 3 November 2026 (the date of the design document) | Agentproof supports Windows from the first version |
| 3 November 2026 | The maintainer tests the plugin in the real Claude desktop app on Windows |
| 3 November 2026 | Only the user settings file can turn on the AI check. The API key is only in an environment variable. The AI check is off by default |
| 3 November 2026 | The engine sends plain HTTP requests to the AI provider, because Rust has no official Claude SDK |
| 3 November 2026 | The free AI check uses the Claude plan of the user through the agentproof-ai agent hook. The engine does not start `claude -p` before Anthropic gives written permission |
| Not recorded | The GitHub repository is private for now. It becomes public at the soft launch (B10) |
| Not recorded | The plugin manifests give the license "MIT OR Apache-2.0". The license files are not in the repository yet (B3) |
| 4 November 2026 | The product is not ready for production use. The analysis recommends Phase A first |
| 4 November 2026 | The maintainer confirmed the start of Phase A, after the documents in this folder are complete |

### Open decisions

| Open decision | Who decides | When the plan needs the answer |
|---|---|---|
| The result of the name check on GitHub, npm, PyPI, plugin marketplaces, domains and trademarks | The maintainer | Before the soft launch, in B9 (week 8) |
| Can a plugin start `claude -p` with the Claude plan of the user? Ask Anthropic for a written answer | The maintainer asks, and Anthropic answers | Before the engine starts `claude -p`. No item in this plan needs the answer. If Anthropic agrees, `claude -p` replaces the agent hook of agentproof-ai |
| The default AI model after the tests on the labelled set: `claude-haiku-5-5`, `claude-sonnet-5-5` or `claude-opus-5-5` | The maintainer, from the precision on the labelled set | In N1, after B7 makes the labelled set |
| The first marketplace: the Claude plugin directory or the self-hosted marketplace | The maintainer | Before the soft launch (B10, end of week 8) |
| How the engine reaches users: binaries in the plugin, or a download at the first run | The maintainer | Before B1 (week 5). B1 proposes binaries in the plugin release. For a download, use a fixed version. Check the checksum or the signature. Print one line that tells what the plugin downloads. Declare the download in the data-handling form of the directory |
| How much of the report a plugin can show inside the desktop app | The maintainer | No item needs it now |
| The default AI provider when the user has several keys | The maintainer | No item needs it now. Today the user settings file must name the provider, so the maintainer can possibly close this question |
| Use the `claude-transcript` crate, or write a new parser | The maintainer | The code has its own parser in `engine/src/transcript.rs`. The maintainer can close this question |
| MIT or Apache-2.0 | The maintainer | The manifests say "MIT OR Apache-2.0", and B3 adds the license files. The maintainer can close this question |
| Confirm the targets, and the precision target for month 12: 90% (section 3) or 95% (section 9) | The maintainer | After the first month of data |
| Where to put the gaps that the code review of 4 November 2026 found and that no item covers. Example: the agent can run `agentproof check` with its Bash tool. Then later hook messages do not list those findings, and strict mode does not stop the agent for them. Another example: a project settings file with `strict = false` turns off the strict mode of the user. Also, the report footer always says that Agentproof sent no data to a server, even after an AI check | The maintainer | Before the soft launch |

## Main risks to this plan

These are the 8 risks in [section 11 of the analysis](product-analysis.md#11-risks) with a high impact and a medium or high likelihood. The full table has all 15 risks, with the plan for each risk.

| Risk | Likelihood, impact | What to do |
|---|---|---|
| Anthropic builds a native claim check | High, High | Support Codex (N10) and CI (N7), because the feature of one vendor cannot work in these places. Keep evidence that a prompt cannot give. Examples are test output, the Git changes since the start point and proof runs. Use the native hooks: `ConfigChange` and `InstructionsLoaded` for sensitive paths, `PostToolUseFailure` for repeated errors, and `SubagentStop` for subagent claims. If Anthropic adds a native claim check, the next release will make Codex and the CI step the main features. Agentproof then reads the native result as an input |
| Because of false alarms, users stop the use of Agentproof | High, High | Use the precision gate for Critical findings: 90% on at least 30 positives (B8). Also use the labelled set (B7), accept once (N13) and news-only messages. Stop the agent only for proven Critical findings |
| A change to the Claude Code transcript format breaks the parser | High, High | Use the health gate (A3). Keep sample transcripts from each Claude Code version in CI (B3). Give "incomplete" on unknown lines |
| One maintainer cannot do all of this | High, High | Obey the list "What we will not do". Keep each phase to approximately 4 weeks of work. Put one finished feature in each release. Use CI tests and contributions with fixtures |
| A funded rival (Entire) adds fact-based claim checks | Medium, High | Read Entire checkpoints as input (L3). Stay local, free and with no account. Release complete and detailed checks first |
| Agentproof itself leaks secrets | Medium, High | Remove secrets from all saved files (A4). Test each pattern. List the removed items in the report. Let only the user settings file turn on the AI check. Delete session data after 30 days (B5) |
| Gatekeeper or SmartScreen blocks the engine | Medium, High | On macOS, sign the binaries (N11). Then notarize them (N11). On Windows, sign the binaries with Authenticode (N11). Until then, document the quarantine step and the SmartScreen warning (B1). Test the install on GitHub Actions macOS and Windows runners (B1) |
| The proof run runs code from the repository | Medium, High | Only the user settings file can turn on the proof run (N12, L1). Ask the user for consent for each project. Use a sandbox with no network. Never turn on the proof run by default |

## How to update this file

1. When an item starts, set its status to "In progress".
2. When the "done when" text of an item is true, set its status to "Done". Add the date and the commit, for example "Done, 20 November 2026, commit `<hash>`".
3. When an item cannot continue, set its status to "Blocked". Give the reason.
4. Give a new item the next unused ID in its phase.
5. Never reuse or change an ID.
6. Write all text in ASD-STE100. The [rules for these documents](README.md#rules-for-these-documents) apply.
7. Add a line to the change log for each change of the plan.

## Change log

- 4 November 2026: first version, from the product analysis.
- 4 November 2026: second version, after 3 reviews (code accuracy, source fidelity and ASD-STE100). New gate condition: items B1 to B9 have the status Done before the soft launch.
- 4 November 2026: Phase A started. A1 is done. The section "A1 result" gives the changes, the test results and the limits that stay.
- 4 November 2026: the documents and A1 are in commit `f95d880`.
- 4 November 2026: A2 is done. The section "A2 result" gives the changes, the test results and the limits that stay.
- 5 November 2026: A2 is in commit `3dd5c19`.
- 5 November 2026: A3 is done. The section "A3 result" gives the changes, the test results and the limits that stay.
- 5 November 2026: A3 is in commit `947d40e`.
- 5 November 2026: A4 is done. The section "A4 result" gives the changes, the test results and the limits that stay.
- 5 November 2026: A4 is in commit `9947e13`.
- 5 November 2026: A5 is done. The section "A5 result" gives the changes, the test results and the limits that stay.
- 5 November 2026: A5 is in commit `6d8f738`.
- 5 November 2026: A6 is done. The section "A6 result" gives the changes, the test results and the limits that stay.
