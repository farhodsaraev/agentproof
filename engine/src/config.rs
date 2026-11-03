//! Settings.
//!
//! - User settings: `~/.agentproof/config.toml`. Only this file can turn on the
//!   AI check, because it says where code is sent and which API key is used.
//! - Project settings: `.agentproof.toml` in the repository. It can turn on
//!   strict mode, but it can never change the AI provider, the address or the
//!   key. A cloned repository must not be able to send your code or your key
//!   to a server that it chooses.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::store;

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub ai: Option<AiConfig>,
    /// Stop the agent when the verdict is "do not trust this work".
    pub strict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Anthropic,
    /// Any server with the OpenAI chat completions API, for example OpenAI or Ollama.
    OpenaiCompatible,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    pub provider: Provider,
    #[serde(default)]
    pub model: Option<String>,
    /// For example `http://localhost:11434/v1` for Ollama.
    #[serde(default)]
    pub base_url: Option<String>,
    /// The environment variable with the API key. The key itself is never in a file.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Anthropic only: `low`, `medium`, `high`, `xhigh` or `max`.
    #[serde(default)]
    pub effort: Option<String>,
}

pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-opus-5-5";
const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";

impl AiConfig {
    pub fn model(&self) -> Result<String> {
        match (&self.model, &self.provider) {
            (Some(m), _) => Ok(m.clone()),
            (None, Provider::Anthropic) => Ok(DEFAULT_ANTHROPIC_MODEL.into()),
            (None, Provider::OpenaiCompatible) => {
                bail!("the AI settings need a model for this provider")
            }
        }
    }

    pub fn base_url(&self) -> String {
        let url = self
            .base_url
            .clone()
            .unwrap_or_else(|| match self.provider {
                Provider::Anthropic => DEFAULT_ANTHROPIC_URL.into(),
                Provider::OpenaiCompatible => DEFAULT_OPENAI_URL.into(),
            });
        url.trim_end_matches('/').to_string()
    }

    /// The API key from the environment. None is allowed for a local server such as Ollama.
    pub fn api_key(&self) -> Result<Option<String>> {
        let name = self
            .api_key_env
            .clone()
            .unwrap_or_else(|| match self.provider {
                Provider::Anthropic => "ANTHROPIC_API_KEY".into(),
                Provider::OpenaiCompatible => "OPENAI_API_KEY".into(),
            });
        match std::env::var(&name) {
            Ok(k) if !k.trim().is_empty() => Ok(Some(k.trim().to_string())),
            _ if self.provider == Provider::Anthropic => {
                bail!("the environment variable {name} has no API key")
            }
            _ => Ok(None),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct UserFile {
    ai: Option<AiConfig>,
    #[serde(default)]
    strict: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    #[serde(default)]
    strict: Option<bool>,
}

pub fn load(repo_root: Option<&Path>) -> Result<Config> {
    let user_path = store::data_dir()?.join("config.toml");
    let user: UserFile = read_toml(&user_path)?.unwrap_or_default();
    let project: ProjectFile = match repo_root {
        Some(root) => read_toml(&root.join(".agentproof.toml"))?.unwrap_or_default(),
        None => ProjectFile::default(),
    };
    Ok(Config {
        ai: user.ai,
        strict: project.strict.or(user.strict).unwrap_or(false),
    })
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map(Some)
            .with_context(|| format!("the settings file {} has an error", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_file_cannot_set_the_ai_provider() {
        let err = toml::from_str::<ProjectFile>(
            "[ai]\nprovider = \"anthropic\"\nbase_url = \"https://evil.example\"",
        );
        assert!(err.is_err());
        assert_eq!(
            toml::from_str::<ProjectFile>("strict = true")
                .unwrap()
                .strict,
            Some(true)
        );
    }

    #[test]
    fn reads_user_ai_settings_with_defaults() {
        let f: UserFile = toml::from_str("[ai]\nprovider = \"anthropic\"").unwrap();
        let ai = f.ai.unwrap();
        assert_eq!(ai.model().unwrap(), "claude-opus-5-5");
        assert_eq!(ai.base_url(), "https://api.anthropic.com");

        let f: UserFile = toml::from_str(
            "[ai]\nprovider = \"openai-compatible\"\nmodel = \"llama3\"\nbase_url = \"http://localhost:11434/v1/\"",
        )
        .unwrap();
        let ai = f.ai.unwrap();
        assert_eq!(ai.base_url(), "http://localhost:11434/v1");
        assert_eq!(ai.model().unwrap(), "llama3");
    }

    #[test]
    fn unknown_keys_are_errors() {
        assert!(
            toml::from_str::<UserFile>("[ai]\nprovider = \"anthropic\"\napi_key = \"sk-x\"")
                .is_err()
        );
    }
}
