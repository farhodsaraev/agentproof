//! The data that Claude Code sends to a hook on standard input.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct HookInput {
    pub session_id: Option<String>,
    pub transcript_path: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
}

impl HookInput {
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(raw).context("the hook data is not valid JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stop_hook_data_and_ignores_unknown_fields() {
        let raw = r#"{"session_id":"abc","transcript_path":"C:/x/abc.jsonl","cwd":"C:/repo",
                      "hook_event_name":"Stop","last_assistant_message":"Done."}"#;
        let input = HookInput::parse(raw).unwrap();
        assert_eq!(input.session_id.as_deref(), Some("abc"));
        assert_eq!(input.cwd, Some(PathBuf::from("C:/repo")));
    }

    #[test]
    fn empty_input_is_allowed() {
        assert!(HookInput::parse("  ").unwrap().session_id.is_none());
    }
}
