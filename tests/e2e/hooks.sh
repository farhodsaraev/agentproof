#!/bin/bash
# End-to-end test of the Agentproof plugin on Windows (Git Bash, like the hooks).
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
S="$(cygpath -m "${TMPDIR:-/tmp}")/agentproof-e2e/hooks"
L="$ROOT/plugin/bin/agentproof"
DATA="$ROOT/engine/tests/data"
rm -rf "$S"; mkdir -p "$S/home"
export AGENTPROOF_HOME="$S/home"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); echo "PASS  $1"; }
bad()  { FAIL=$((FAIL+1)); echo "FAIL  $1"; [ -n "$2" ] && echo "      got: $(echo "$2" | head -c 400)"; }
has()  { case "$1" in *"$2"*) return 0;; *) return 1;; esac; }

# A repository with a space and a non-ASCII letter in its path.
R="$S/my repo é"
mkdir -p "$R/src"
( cd "$R" && git init -q && git config user.email t@e.com && git config user.name T && git config core.autocrlf false
  printf 'pub fn login() -> bool {\n    true\n}\n\n#[test]\nfn login_works() {\n    assert!(login());\n}\n' > src/login.rs
  printf '[package]\nname = "demo"\n\n[dependencies]\n' > Cargo.toml
  printf 'pub fn charge() {}\n' > src/billing.rs
  git add . && git commit -qm first )
T="$S/t.jsonl"
input() { printf '{"session_id":"%s","transcript_path":"%s","cwd":"%s","hook_event_name":"%s","stop_hook_active":false}' "$1" "$T" "$R" "$2"; }

echo "== 1. Session start"
cp "$DATA/good_agent.jsonl" "$T"
sed -i 's#src/lib.rs#src/billing.rs#g' "$T"
out=$(input s1 SessionStart | "$L" start --hook); code=$?
[ $code -eq 0 ] && [ -z "$out" ] && ok "start hook prints nothing and exits 0" || bad "start hook output/exit ($code)" "$out"
D="$AGENTPROOF_HOME/$(ls "$AGENTPROOF_HOME")/s1"
[ -f "$D/start.json" ] && ok "start point saved outside the repository" || bad "no start.json"
[ -z "$(cd "$R" && git status --porcelain)" ] && ok "repository unchanged after start" || bad "repository changed"

echo "== 2. No change"
out=$(input s1 Stop | "$L" check --hook)
[ -z "$out" ] && ok "no change: no message" || bad "no change produced output" "$out"

echo "== 3. Good agent"
printf 'pub fn charge() {}\n\npub fn refund() {}\n' > "$R/src/billing.rs"
sed -i 's#Add a greeting function to src/billing.rs#Add a refund function to src/billing.rs#' "$T"
out=$(input s1 Stop | "$L" check --hook)
has "$out" '"systemMessage"' && has "$out" 'no red flags found' && ok "good work: no red flags" || bad "good work verdict" "$out"
has "$out" "so this change is expected" && ok "requested sensitive file is Low with a note" || bad "requested sensitive file note" "$out"
out2=$(input s1 Stop | "$L" check --hook)
[ -z "$out2" ] && ok "same state again: no message" || bad "repeated message" "$out2"
# start again (resume) must keep the first start point
before=$(cat "$D/start.json"); input s1 SessionStart | "$L" start --hook; after=$(cat "$D/start.json")
[ "$before" = "$after" ] && ok "resume keeps the first start point" || bad "start point replaced on resume"

echo "== 4. Bad agent"
cp "$DATA/bad_agent.jsonl" "$T"
printf '#[allow(dead_code)]\npub fn login() -> bool {\n    true\n}\n\n#[test]\n#[ignore]\nfn other() {}\n' > "$R/src/login.rs"
printf '[package]\nname = "demo"\n\n[dependencies]\nleftpad = "1"\n' > "$R/Cargo.toml"
( cd "$R" && git commit -qam wip )
out=$(input s1 Stop | "$L" check --hook)
has "$out" 'do not trust this work' && ok "bad work: do not trust" || bad "bad work verdict" "$out"
has "$out" 'New. Critical' && ok "new findings are marked New" || bad "no New mark" "$out"
has "$out" 'tests failed, but the agent said that the tests pass' && ok "failed tests with a pass claim found" || bad "missing failed-tests claim" "$out"
J="$D/check.json"
for rule in tests_failed_but_claimed_pass dangerous_command test_skipped dependencies_changed sensitive_file suppressions_added repeated_error; do
  grep -q "\"rule\": \"$rule\"" "$J" && ok "check.json has $rule" || bad "check.json misses $rule"
done
out2=$(input s1 Stop | "$L" check --hook)
[ -z "$out2" ] && ok "same findings again: no message" || bad "findings repeated" "$out2"

echo "== 5. Strict mode"
printf 'strict = true\n' > "$R/.agentproof.toml"
rm_line='{"type":"assistant","timestamp":"2026-11-03T10:03:20.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"r1","name":"Bash","input":{"command":"rm -rf src/old"}}]}}'
head -n 11 "$DATA/bad_agent.jsonl" > "$T"; echo "$rm_line" >> "$T"; tail -n 1 "$DATA/bad_agent.jsonl" >> "$T"
out=$(input s1 Stop | "$L" check --hook)
has "$out" '"decision":"block"' && has "$out" 'recursive delete' && ok "strict mode blocks a new critical finding" || bad "strict block" "$out"
has "$out" 'force push' && bad "strict block repeats an old finding" "$out" || ok "strict block has only the new finding"
out=$(input s1 Stop | "$L" check --hook)
has "$out" '"decision"' && bad "strict mode blocks twice" "$out" || ok "strict mode blocks only once"
rm "$R/.agentproof.toml"

echo "== 6. Report and JSON"
H="$D/report.html"
for part in "Do not trust this work" "What to verify" "Red flags" "Claimed and done" "Blast radius" "What happened" "The AI check is off"; do
  grep -q "$part" "$H" && ok "report has: $part" || bad "report misses: $part"
done
grep -q -E '(src|href)="https?:' "$H" && bad "report loads external content" || ok "report has no external requests"
powershell -NoProfile -Command "try { Get-Content '$(cygpath -w "$J")' -Raw | ConvertFrom-Json | Out-Null; 'ok' } catch { 'bad' }" | grep -q ok && ok "check.json is valid JSON" || bad "check.json invalid"
[ -z "$(cd "$R" && git status --porcelain --ignored | grep -v -E 'src/|Cargo|\.agentproof')" ] && ok "no Agentproof files inside the repository" || bad "files written in repository" "$(cd "$R" && git status --porcelain --ignored)"

echo "== 7. Manual check"
out=$("$L" check --cwd "$R" --session s1 --transcript "$T")
has "$out" 'Agentproof:' && has "$out" 'force push' && ok "manual check shows all findings" || bad "manual check" "$out"

echo "== 8. Errors"
N="$S/not a repo"; mkdir -p "$N"
out=$(printf '{"session_id":"n1","transcript_path":"%s","cwd":"%s"}' "$T" "$N" | "$L" check --hook)
has "$out" 'not a Git repository' && ok "folder without Git: clear message" || bad "no-git message" "$out"
out=$(printf '{"session_id":"m1","transcript_path":"%s/missing.jsonl","cwd":"%s"}' "$S" "$R" | "$L" check --hook)
has "$out" 'cannot read the session transcript' && has "$out" 'incomplete' && ok "missing transcript: incomplete verdict" || bad "missing transcript" "$out"
out=$(echo 'not json' | "$L" check --hook 2>"$S/err.txt"); code=$?
[ $code -ne 0 ] && [ $code -ne 2 ] && [ -z "$out" ] && grep -q "cannot examine" "$S/err.txt" && ok "bad hook input: non-blocking error (exit $code)" || bad "bad input handling (exit $code)" "$out $(cat "$S/err.txt")"
out=$(printf '{"session_id":"../../x","transcript_path":"%s","cwd":"%s"}' "$T" "$R" | "$L" check --hook 2>&1); code=$?
[ $code -eq 1 ] && has "$out" 'not valid' && ok "unsafe session id rejected" || bad "unsafe session id" "$out"

echo "== 9. Speed on a large real transcript"
BIG=$(ls -S ~/.claude/projects/*/*.jsonl 2>/dev/null | head -1)
[ -n "$BIG" ] || { echo "SKIP  speed test: no Claude Code transcript on this computer"; echo; echo "RESULT: $PASS passed, $FAIL failed"; exit $((FAIL > 0)); }
size=$(du -k "$BIG" | cut -f1)
s=$(date +%s%N); "$L" check --cwd "$R" --session big1 --transcript "$BIG" >/dev/null; e=$(date +%s%N)
ms=$(( (e-s)/1000000 ))
[ $ms -lt 5000 ] && ok "check of a ${size} KB transcript in ${ms} ms" || bad "slow check: ${ms} ms for ${size} KB"

echo
echo "RESULT: $PASS passed, $FAIL failed"
exit $((FAIL > 0))
