#!/bin/bash
# End-to-end test of the AI check on Windows, with a fake Anthropic server.
HERE="$(cygpath -m "$(cd "$(dirname "$0")" && pwd)")"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
S="$(cygpath -m "${TMPDIR:-/tmp}")/agentproof-e2e/ai"
L="$ROOT/plugin/bin/agentproof"
rm -rf "$S"; mkdir -p "$S/home" "$S/srv"
export AGENTPROOF_HOME="$S/home"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "PASS  $1"; }
bad() { FAIL=$((FAIL+1)); echo "FAIL  $1"; [ -n "$2" ] && echo "      got: $(echo "$2" | head -c 500)"; }
has() { case "$1" in *"$2"*) return 0;; *) return 1;; esac; }
wait_for() { for _ in $(seq 1 60); do [ -e "$1" ] && return 0; sleep 0.5; done; return 1; }

powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w "$HERE/fake-anthropic.ps1")" -Port 8799 -Count 2 -Out "$(cygpath -w "$S/srv")" &
SERVER=$!
wait_for "$S/srv/ready.txt" && ok "fake server started" || { bad "fake server did not start"; exit 1; }

export AP_E2E_KEY="e2e-test-key-123"
printf '[ai]\nprovider = "anthropic"\nbase_url = "http://localhost:8799"\napi_key_env = "AP_E2E_KEY"\n' > "$AGENTPROOF_HOME/config.toml"

R="$S/repo"; mkdir -p "$R/src"
( cd "$R" && git init -q && git config user.email t@e.com && git config user.name T && git config core.autocrlf false
  printf 'pub fn login() -> bool {\n    false\n}\n' > src/login.rs
  git add . && git commit -qm first )
T="$S/t.jsonl"
cp "$ROOT/engine/tests/data/bad_agent.jsonl" "$T"
input() { printf '{"session_id":"%s","transcript_path":"%s","cwd":"%s"}' "$1" "$T" "$R"; }
input a1 | "$L" start --hook

# The agent changes code and plants secrets that must never leave the computer.
printf 'pub fn login() -> bool {\n    true\n}\nconst API_KEY: &str = "sk-ant-api03-PLANTEDSECRETPLANTEDSECRET1234";\n' > "$R/src/login.rs"
printf 'DB_PASSWORD=hunter2hunter2\n' > "$R/.env"

echo "== 1. Manual check waits for the AI check"
out=$("$L" check --cwd "$R" --session a1 --transcript "$T")
has "$out" 'AI opinion: the evidence does not agree with the claim "All tests pass"' && ok "AI finding in the message" || bad "AI finding missing" "$out"
REQ="$S/srv/request1.txt"
[ -f "$REQ" ] && ok "request reached the server" || bad "no request"
grep -qi "^x-api-key: e2e-test-key-123" "$REQ" && ok "API key sent in the header" || bad "API key header"
grep -qi "^anthropic-version: 2026-10-21" "$REQ" && ok "API version header" || bad "API version header"
grep -qi "^anthropic-beta: server-side-fallback-2026-07-01" "$REQ" && ok "fallback beta header" || bad "fallback header"
grep -q '"model":"claude-opus-5-5"' "$REQ" && ok "default model claude-opus-5-5" || bad "model" "$(grep -o '"model":"[^"]*"' "$REQ")"
grep -q '"fallbacks":"default"' "$REQ" && ok "fallbacks default" || bad "fallbacks"
grep -q '"json_schema"' "$REQ" && ok "structured output schema" || bad "schema"
grep -q 'PLANTEDSECRET' "$REQ" && bad "secret key was sent" || ok "planted API key was removed"
grep -q 'hunter2' "$REQ" && bad ".env content was sent" || ok ".env content was not sent"
grep -q 'e2e-test-key-123' <(tail -n +2 "$REQ" | sed -n '/^$/,$p') && bad "API key in the request body" || ok "API key not in the request body"
D="$AGENTPROOF_HOME/$(ls "$AGENTPROOF_HOME" | grep -v config)/a1"
H="$D/report.html"
grep -q "Contradicted" "$H" && grep -q "Supported" "$H" && ok "report shows AI verdicts" || bad "report AI table"
grep -q "Agentproof removed" "$H" && ok "report counts removed secrets" || bad "report removed-secrets note"
grep -q -E "hunter2|PLANTEDSECRET" "$H" && bad "report shows a secret" || ok "report shows no planted secret"
grep -q -E "hunter2|PLANTEDSECRET" "$D/check.json" && echo "NOTE  check.json (local only) contains a planted secret in the session data" || ok "check.json has no planted secret"
grep -q "rule checks and the AI check" "$H" && ok "report says that the AI check ran" || bad "report AI note"

cp "$H" "$S/report-ai-done.html"
echo "== 2. Same input: no second AI call"
"$L" check --cwd "$R" --session a1 --transcript "$T" >/dev/null
[ ! -f "$S/srv/request2.txt" ] && ok "saved AI result is used again" || bad "AI called again for the same input"

echo "== 3. Hook does not wait; background AI check"
printf 'pub fn login() -> bool {\n    true && true\n}\n' > "$R/src/login.rs"
s=$(date +%s%N); out=$(input a1 | "$L" check --hook); e=$(date +%s%N); ms=$(( (e-s)/1000000 ))
[ $ms -lt 4000 ] && ok "hook returned in ${ms} ms" || bad "hook waited ${ms} ms"
has "$out" "The AI check runs now" && ok "message says that the AI check runs" || bad "running note" "$out"
wait_for "$S/srv/request2.txt" && ok "background process called the AI provider" || bad "no background request"
for _ in $(seq 1 40); do [ ! -e "$D/ai.lock" ] && break; sleep 0.5; done
[ ! -e "$D/ai.lock" ] && ok "background process removed its lock" || bad "lock file remains"
grep -q '"status": "done"' "$D/check.json" && ok "check.json has the AI result" || bad "check.json AI status" "$(grep -o '"status": "[a-z_]*"' "$D/check.json")"
out=$(input a1 | "$L" check --hook)
[ -z "$out" ] || has "$out" "AI opinion" && ok "next hook message is quiet or shows the AI news" || bad "next hook message" "$out"

echo "== 4. Wrong key name: clear error, no crash"
printf '[ai]\nprovider = "anthropic"\napi_key_env = "AP_E2E_MISSING"\n' > "$AGENTPROOF_HOME/config.toml"
echo x >> "$R/src/login.rs"
"$L" check --cwd "$R" --session a1 --transcript "$T" >/dev/null; code=$?
[ $code -eq 0 ] && grep -q "AP_E2E_MISSING" "$D/check.json" && ok "missing key: error recorded, check still done" || bad "missing key handling (exit $code)"

echo "== 5. A repository cannot turn on the AI check"
rm "$AGENTPROOF_HOME/config.toml"
printf '[ai]\nprovider = "anthropic"\nbase_url = "https://evil.example"\n' > "$R/.agentproof.toml"
echo y >> "$R/src/login.rs"
out=$("$L" check --cwd "$R" --session a1 --transcript "$T")
grep -q '"status": "off"' "$D/check.json" && ok "AI check stays off" || bad "repository file turned on the AI check"
grep -q "settings file" "$D/check.json" && ok "the error in the project file is reported" || bad "no note about the project file"

kill $SERVER 2>/dev/null
echo
echo "RESULT: $PASS passed, $FAIL failed"
exit $((FAIL > 0))
