# Agentproof

Your agent says "done." Agentproof shows the proof.

Agentproof is a plugin for Claude Code. It examines the work of an AI coding agent after each turn. It compares the session transcript with the changes in the repository.

**Status: milestone 4 of 4.** Agentproof runs ten rule checks and an optional AI check, gives a verdict and writes a full HTML report.

## What Agentproof does now

- When a session starts, Agentproof records the state of the repository.
- When the agent stops, Agentproof finds all changes since the start. These include commits that the agent made.
- Agentproof keeps changes from before the session apart from the work of the agent.
- Agentproof shows a short message in Claude Code. The message shows only new findings.
- Agentproof writes a full HTML report and a JSON file. The report has no external requests, so you can attach it to a pull request.
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

To examine the latest session manually, use the command `/agentproof:check`. To open the report in your web browser, use the command `/agentproof:report`.

## Turn on the AI check

The AI check compares the claims of the agent with the evidence. It is off until you turn it on. When it is on, Agentproof sends the requests, the final message, the commands and the changes to the AI provider that you choose. Agentproof removes secrets before it sends them.

1. Get an API key from your AI provider.
2. Put the key in an environment variable. Do not put the key in a file. On Windows, for Anthropic:
   ```
   setx ANTHROPIC_API_KEY "your-key"
   ```
3. Make the file `~/.agentproof/config.toml` with this text:
   ```toml
   [ai]
   provider = "anthropic"
   # model = "claude-opus-5-5"   (the default)
   # effort = "medium"           (low, medium, high, xhigh or max)
   ```
   For OpenAI or a local server such as Ollama, use `provider = "openai-compatible"`, a `model`, and a `base_url`, for example `http://localhost:11434/v1`.
4. Close Claude Code fully, and start it again. Then it can read the new environment variable.

Only your user settings file can turn on the AI check. A `.agentproof.toml` file in a repository cannot change the provider, the address or the key.

The AI check runs in the background after each turn with changes, so the agent does not wait. The report shows the result when it is complete. The next message shows new AI findings. A manual check (`/agentproof:check`) waits for the AI check.

## AI check with your Claude plan (no API key)

The optional plugin `agentproof-ai` adds an AI check that uses your own Claude Code login and plan. It needs no API key and costs no extra money. It uses some of the limits of your plan.

How it operates:

- When the agent stops, Claude Code starts a separate checker agent. The checker did not write the work.
- The checker reads the final message of the agent and finds the claims about the work.
- It looks for evidence in the files, in the transcript and in `git diff`. It does not change files or run tests.
- If the evidence shows that a claim is false, the agent gets the reason. Then the agent must correct the work or tell you.
- The checker examines each turn one time only, so it cannot keep the agent in a loop.

Limits:

- The result goes to the agent and into the chat. It is not in the Agentproof report. For the AI result in the report, use the API key method above.
- Claude Code marks this hook type as experimental.
- The checker starts after each turn. If the final message has no claims about the work, it stops immediately.

To install it:

```
/plugin install agentproof-ai@agentproof
```

Do not turn on both AI methods at the same time, because then each turn uses two AI checks.

## Strict mode

In strict mode, Agentproof stops the agent when it finds a new critical problem. The agent then gets the findings and must correct them or tell you about them. In strict mode, the AI check runs before the agent stops.

To turn on strict mode for one project, add this line to `.agentproof.toml` in the repository:

```toml
strict = true
```

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
| `plugin/` | The Claude Code plugin: hooks, commands and launcher. |
| `plugin-ai/` | The optional plugin for the AI check with your Claude plan. |
| `.claude-plugin/` | The local plugin marketplace. |
| `scripts/` | Build scripts. |
| `.devcontainer/` | The development container. |

## Text rules

All text that Agentproof shows to users follows ASD-STE100 Simplified Technical English. A test in `engine/src/messages.rs` examines the length of each sentence.
