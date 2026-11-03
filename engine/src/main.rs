//! Agentproof engine. The plugin hooks call this program.

mod changes;
mod check;
mod checks;
mod hook;
mod messages;
mod store;
mod time;
mod transcript;

use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "agentproof",
    version,
    about = "Checks whether to trust what an AI coding agent did."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Record the start point of a session. The session-start hook calls this.
    Start {
        /// Read the hook data from standard input.
        #[arg(long)]
        hook: bool,
        #[command(flatten)]
        target: Target,
    },
    /// Check a session. The stop hook calls this with --hook.
    Check {
        /// Read the hook data from standard input and reply in hook format.
        #[arg(long)]
        hook: bool,
        /// Print the full result as JSON.
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        target: Target,
    },
}

#[derive(clap::Args, Default)]
struct Target {
    /// Session id. Default: the latest session for the project folder.
    #[arg(long)]
    session: Option<String>,
    /// Path to the session transcript file.
    #[arg(long)]
    transcript: Option<PathBuf>,
    /// Project folder. Default: the current folder.
    #[arg(long)]
    cwd: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Start { hook, target } => run_start(hook, target),
        Command::Check { hook, json, target } => run_check(hook, json, target),
    };
    if let Err(err) = result {
        // A failure must never stop the agent. Tell the user and exit normally.
        eprintln!("{}", messages::engine_error(&format!("{err:#}")));
        std::process::exit(1);
    }
}

fn resolve(hook: bool, target: Target) -> Result<hook::HookInput> {
    let mut input = if hook {
        let mut raw = String::new();
        std::io::stdin().read_to_string(&mut raw)?;
        hook::HookInput::parse(&raw)?
    } else {
        hook::HookInput::default()
    };
    if let Some(s) = target.session {
        input.session_id = Some(s);
    }
    if let Some(t) = target.transcript {
        input.transcript_path = Some(t);
    }
    if let Some(c) = target.cwd {
        input.cwd = Some(c);
    }
    if input.cwd.is_none() {
        input.cwd = Some(std::env::current_dir()?);
    }
    Ok(input)
}

fn run_start(hook: bool, target: Target) -> Result<()> {
    let input = resolve(hook, target)?;
    let cwd = input.cwd.clone().unwrap_or_default();
    let session = match input.session_id.clone() {
        Some(s) => s,
        None => store::latest_transcript(&cwd)?.0,
    };
    // Print nothing: Claude Code adds session-start output to the agent's context.
    check::record_start(&cwd, &session)
}

fn run_check(hook: bool, json: bool, target: Target) -> Result<()> {
    let input = resolve(hook, target)?;
    let outcome = check::run(&input, !hook)?;
    let Some(result) = outcome else {
        // Nothing changed since the last check.
        return Ok(());
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if hook {
        println!("{}", serde_json::json!({ "systemMessage": result.message }));
    } else {
        println!("{}", result.message);
    }
    Ok(())
}
