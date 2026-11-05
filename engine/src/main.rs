//! Agentproof engine. The plugin hooks call this program.

mod ai;
mod changes;
mod check;
mod checks;
mod config;
mod hook;
mod messages;
mod redact;
mod report;
mod store;
mod testruns;
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
    /// Run the AI check and update the report. The stop hook starts this in the background.
    #[command(hide = true)]
    Ai {
        #[command(flatten)]
        target: Target,
    },
    /// Open the report of the latest check in the web browser.
    Open {
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
        Command::Open { target } => run_open(target),
        Command::Ai { target } => run_ai(target),
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
    let opts = check::Options {
        force: !hook,
        hook,
        // A manual check waits for the AI check. The hook does not.
        ai_now: !hook,
        background: false,
    };
    let result = check::run(&input, opts)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&store::clean_json(&result)?)?
        );
    } else if hook {
        let mut out = serde_json::Map::new();
        if let Some(message) = &result.message {
            out.insert("systemMessage".into(), message.clone().into());
        }
        // Strict mode: Claude Code does not let the agent stop, and gives it the reason.
        if let Some(reason) = &result.block_reason {
            out.insert("decision".into(), "block".into());
            out.insert("reason".into(), reason.clone().into());
        }
        if !out.is_empty() {
            println!("{}", serde_json::Value::Object(out));
        }
    } else if let Some(message) = &result.message {
        println!("{message}");
    }
    Ok(())
}

/// The background AI check. The stop hook starts it.
fn run_ai(target: Target) -> Result<()> {
    let input = resolve(false, target)?;
    let opts = check::Options {
        ai_now: true,
        background: true,
        ..check::Options::default()
    };
    let result = check::run(&input, opts);
    check::end_background(&input);
    result.map(|_| ())
}

fn run_open(target: Target) -> Result<()> {
    let input = resolve(false, target)?;
    let cwd = input.cwd.clone().unwrap_or_default();
    let session = match input.session_id.clone() {
        Some(s) => s,
        None => store::latest_transcript(&cwd)?.0,
    };
    let path = store::session_dir(&cwd, &session)?.join("report.html");
    if !path.exists() {
        // Make the report first, without a message.
        check::run(&input, check::Options::default())?;
    }
    report::open(&path)?;
    println!(
        "{}",
        messages::report_opened(&path.to_string_lossy().replace('\\', "/"))
    );
    Ok(())
}
