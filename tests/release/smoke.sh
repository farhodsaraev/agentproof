#!/bin/bash
# Release test: start the plugin engine through the plugin launcher and check
# a fixture session (audit experiment e1). The release workflow runs this on
# Windows, macOS and Linux before it publishes a release.
#
# Usage: bash tests/release/smoke.sh <plugin folder>
# The plugin folder must have the engine files in engine/<target>/.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PLUGIN="$(cd "${1:?give the plugin folder}" && pwd)"
L="$PLUGIN/bin/agentproof"
EXP="$ROOT/engine/tests/experiments"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "PASS  $1"; }
bad() { FAIL=$((FAIL+1)); echo "FAIL  $1"; [ -n "${2:-}" ] && echo "      got: $(echo "$2" | head -c 600)"; }

# 1. The checksums of the engine files.
if [ -f "$PLUGIN/engine/SHA256SUMS" ]; then
  if command -v sha256sum >/dev/null; then sum="sha256sum"; else sum="shasum -a 256"; fi
  out=$(cd "$PLUGIN/engine" && $sum -c SHA256SUMS 2>&1) && ok "checksums match" || bad "checksums" "$out"
else
  bad "no SHA256SUMS file"
fi

# 2. The launcher finds the engine of this computer.
out=$("$L" --version 2>&1) && ok "launcher starts the engine: $out" || bad "launcher" "$out"

# 3. A check of experiment e1 gives "do not trust this work".
W="$(mktemp -d)"
export AGENTPROOF_HOME="$W/home"
R="$W/repo"; mkdir -p "$R/src/auth" "$R/tests" "$R/.github/workflows"
( cd "$R" && git init -q && git config user.email t@e.com && git config user.name T && git config core.autocrlf false
  printf 'export function login(u,p){ return check(u,p); }\n' > src/auth/login.js
  printf 'export function add(a,b){ return a+b; }\n' > src/math.js
  printf 'import {add} from "../src/math";\ntest("adds", () => {\n  expect(add(1,2)).toBe(3);\n});\ntest("adds neg", () => {\n  expect(add(-1,-2)).toBe(-3);\n});\n' > tests/math.test.js
  printf 'name: ci\non: push\njobs:\n  t:\n    runs-on: ubuntu-latest\n    steps:\n      - run: npm test\n' > .github/workflows/ci.yml
  printf '{"name":"demo","scripts":{"test":"jest"}}\n' > package.json
  git add . && git commit -qm first )
# Windows programs need Windows paths.
p() { if command -v cygpath >/dev/null; then cygpath -m "$1"; else echo "$1"; fi; }
"$L" start --session e1 --cwd "$(p "$R")" && ok "start hook command" || bad "start hook command"
git -C "$R" apply "$EXP/e1.patch" && ok "agent changes applied" || bad "patch"
out=$("$L" check --session e1 --transcript "$(p "$EXP/e1.jsonl")" --cwd "$(p "$R")" --json 2>&1)
case "$out" in
  *'"verdict": "dont_trust"'*) ok "verdict: do not trust this work" ;;
  *) bad "verdict" "$out" ;;
esac
case "$out" in
  *'"rule": "tests_failed_but_claimed_pass"'*) ok "finding: tests failed, but the agent said that they pass" ;;
  *) bad "finding" "$out" ;;
esac
out=$("$L" check --session e1 --transcript "$(p "$EXP/e1.jsonl")" --cwd "$(p "$R")" 2>&1)
case "$out" in
  "Agentproof: do not trust this work."*) ok "message for the user" ;;
  *) bad "message" "$out" ;;
esac
rm -rf "$W"

echo
echo "RESULT: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
