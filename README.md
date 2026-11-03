# Agentproof

Your agent says "done." Agentproof shows the proof.

Agentproof is a plugin for Claude Code. It examines the work of an AI coding agent after each turn. It compares the session transcript with the changes in the repository.

**Status: milestone 2 of 4.** Agentproof runs ten rule checks and gives a verdict. It does not have the AI check or the full report yet.

## What Agentproof does now

- When a session starts, Agentproof records the state of the repository.
- When the agent stops, Agentproof finds all changes since the start. These include commits that the agent made.
- Agentproof keeps changes from before the session apart from the work of the agent.
- Agentproof shows a short message in Claude Code. It writes the full data to a JSON file.
- Agentproof runs ten rule checks. Examples: tests that failed although the agent said that they pass, skipped tests, dangerous commands, new dependencies and changes to sensitive files.
- Agentproof gives a verdict: "do not trust this work", "examine N items" or "no red flags found". It never says "safe" without the AI check.
- If nothing changed since the last check, Agentproof shows nothing.
- Agentproof does not change the repository. It keeps its own files in `~/.agentproof`.

## Install the plugin on your computer

You must have Git. On Windows, you must have Git Bash, because the plugin hooks use it.

1. Build the engine (see "Development").
2. In Claude Code, add this folder as a plugin marketplace:
   ```
   /plugin marketplace add C:/path/to/agentproof
   ```
3. Install the plugin:
   ```
   /plugin install agentproof@agentproof
   ```
4. Start a new session. Agentproof starts to operate automatically.

To examine the latest session manually, use the command `/agentproof:check`.

## Development

Development occurs in a Docker container. You do not install Rust on your computer.

1. Open a shell in the container:
   ```
   docker compose run --rm dev
   ```
2. Run the tests:
   ```
   cd engine && cargo test
   ```
3. Build the engine for Linux and Windows, and put it into the plugin:
   ```
   sh scripts/build-plugin.sh
   ```

The container cannot build the macOS version yet.

## Folders

| Folder | Contents |
| --- | --- |
| `engine/` | The Rust engine. It does all the work. |
| `plugin/` | The Claude Code plugin: hooks, command and launcher. |
| `.claude-plugin/` | The local plugin marketplace. |
| `scripts/` | Build scripts. |
| `.devcontainer/` | The development container. |

## Text rules

All text that Agentproof shows to users follows ASD-STE100 Simplified Technical English. A test in `engine/src/messages.rs` examines the length of each sentence.
