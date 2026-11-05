//! Remove secrets from all text before it goes to an AI provider, into a saved
//! file, into the message or into the report.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::transcript::Session;

pub const MARK: &str = "[REMOVED BY AGENTPROOF]";

struct Patterns {
    /// Secrets that are removed completely.
    whole: Vec<Regex>,
    /// `name = value` pairs: the name stays, the value goes.
    assignment: Regex,
    /// `scheme://user:password@host`: the password goes.
    url_password: Regex,
    /// `Bearer <token>`: the token goes.
    bearer: Regex,
}

/// Secrets that are removed completely.
const WHOLE: &[&str] = &[
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(-----END [A-Z ]*PRIVATE KEY-----|$)",
    r"sk-ant-[A-Za-z0-9_\-]{20,}",
    r"sk-(?:proj-)?[A-Za-z0-9_\-]{20,}",
    r"gh[pousr]_[A-Za-z0-9]{30,}",
    r"github_pat_[A-Za-z0-9_]{30,}",
    r"AKIA[0-9A-Z]{16}",
    r"xox[abeprs]-[A-Za-z0-9\-]{10,}",
    // Stripe keys and webhook secrets.
    r"\b(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{10,}",
    r"\bwhsec_[A-Za-z0-9]{16,}",
    // npm and GitLab tokens.
    r"\bnpm_[A-Za-z0-9]{30,}",
    r"\bglpat-[A-Za-z0-9_\-]{20,}",
    r"AIza[0-9A-Za-z_\-]{35}",
    r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
];

/// `NAME=value`, `name: value` and `name = "value"`. A value without quotes
/// after ` = ` is code (`x = form.get(...)`), not a secret.
const ASSIGNMENT: &str = r#"(?i)((?:api[_-]?key|secret|token|passw(?:or)?d|pwd|credential|auth[_-]?key|private[_-]?key)[A-Za-z0-9_\-]*["']?(?:\s*:\s*["']?|=["']?|\s*=\s*["']))([^\s"',;()]{6,})"#;
const URL_PASSWORD: &str = r"([a-z][a-z0-9+.\-]*://[^/\s:@]+:)([^@\s/]+)(@)";
const BEARER: &str = r"(?i)\b(bearer\s+)([A-Za-z0-9._~+/\-]{16,}=*)";

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    let re = |p: &str| Regex::new(p).expect("valid pattern");
    P.get_or_init(|| Patterns {
        whole: WHOLE.iter().map(|p| re(p)).collect(),
        assignment: re(ASSIGNMENT),
        url_password: re(URL_PASSWORD),
        bearer: re(BEARER),
    })
}

/// Returns the text without secrets, and the number of removed secrets.
pub fn redact(text: &str) -> (String, usize) {
    let p = patterns();
    let mut count = 0;
    let mut out = text.to_string();
    for re in &p.whole {
        count += re.find_iter(&out).count();
        out = re.replace_all(&out, MARK).into_owned();
    }
    let mut n = 0;
    out = p
        .assignment
        .replace_all(&out, |c: &regex::Captures| {
            if c[2].starts_with('[') {
                return c[0].to_string(); // Already removed.
            }
            n += 1;
            format!("{}{MARK}", &c[1])
        })
        .into_owned();
    count += n;
    count += p.url_password.find_iter(&out).count();
    out = p
        .url_password
        .replace_all(&out, format!("${{1}}{MARK}${{3}}"))
        .into_owned();
    count += p.bearer.find_iter(&out).count();
    out = p
        .bearer
        .replace_all(&out, format!("${{1}}{MARK}"))
        .into_owned();
    (out, count)
}

/// Remove secrets from a text in place.
fn clean(text: &mut String) {
    let (out, n) = redact(text);
    if n > 0 {
        *text = out;
    }
}

/// Remove secrets from each text of a session: the prompts, the commands, their
/// errors and output, the final message and the steps of the timeline.
pub fn session(s: &mut Session) {
    for p in &mut s.prompts {
        clean(&mut p.text);
    }
    for c in &mut s.commands {
        clean(&mut c.command);
        if let Some(e) = &mut c.error {
            clean(e);
        }
        if let Some(o) = &mut c.output {
            clean(o);
        }
    }
    if let Some(m) = &mut s.final_message {
        clean(m);
    }
    for e in &mut s.events {
        clean(&mut e.text);
    }
}

/// Remove secrets from each text in a JSON value. Each file that the engine
/// saves, and its JSON output, goes through this.
pub fn json(v: &mut Value) {
    match v {
        Value::String(s) => clean(s),
        Value::Array(items) => items.iter_mut().for_each(json),
        Value::Object(map) => map.values_mut().for_each(json),
        _ => {}
    }
}

/// True for files whose whole content is secret, for example `.env`, a private
/// key or `src/auth/secrets.json`. This does not depend on the other sensitive
/// areas: a secret file in an authentication folder is still a secret file.
pub fn secret_file(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    let name = p.rsplit('/').next().unwrap_or(&p);
    if [".example", ".sample", ".template", ".dist"]
        .iter()
        .any(|ext| name.ends_with(ext))
    {
        return false;
    }
    let by_name = name == ".env"
        || name.starts_with(".env.")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || matches!(name, ".npmrc" | ".pypirc" | ".netrc" | ".htpasswd")
        || [".pem", ".key", ".p12", ".pfx", ".jks", ".keystore"]
            .iter()
            .any(|ext| name.ends_with(ext));
    let words = name.split(|c: char| !c.is_ascii_alphanumeric());
    by_name
        || words
            .into_iter()
            .any(|w| matches!(w, "secret" | "secrets" | "credential" | "credentials"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_known_key_formats() {
        let text = "a sk-ant-api03-abcdefghijklmnopqrstuvwxyz012345 b AKIAABCDEFGHIJKLMNOP c ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        let (out, n) = redact(text);
        assert_eq!(n, 3, "{out}");
        assert!(
            !out.contains("sk-ant") && !out.contains("AKIA") && !out.contains("ghp_"),
            "{out}"
        );
    }

    #[test]
    fn removes_values_of_secret_names_but_keeps_the_names() {
        let (out, n) =
            redact("DB_PASSWORD=hunter2hunter\nlet api_key = \"abcdef123456\";\ntoken: xyz98765");
        assert_eq!(n, 3, "{out}");
        assert!(out.contains("DB_PASSWORD=[REMOVED BY AGENTPROOF]"), "{out}");
        assert!(out.contains("api_key = \"[REMOVED BY AGENTPROOF]"), "{out}");
        assert!(
            !out.contains("hunter2") && !out.contains("abcdef123456") && !out.contains("xyz98765")
        );
    }

    #[test]
    fn removes_private_keys_and_url_passwords() {
        let (out, n) = redact("-----BEGIN RSA PRIVATE KEY-----\nMIIEow\n-----END RSA PRIVATE KEY-----\npostgres://app:s3cr3tpw@db:5432/x");
        assert_eq!(n, 2, "{out}");
        assert!(
            !out.contains("MIIEow") && !out.contains("s3cr3tpw"),
            "{out}"
        );
        assert!(
            out.contains("postgres://app:[REMOVED BY AGENTPROOF]@db"),
            "{out}"
        );
    }

    #[test]
    fn keeps_normal_code() {
        let code = "fn token_count(text: &str) -> usize { text.len() }\nlet password_field = form.get(\"password\");";
        assert_eq!(redact(code), (code.to_string(), 0));
    }

    #[test]
    fn knows_secret_files() {
        assert!(secret_file(".env"));
        assert!(secret_file("config/.env.production"));
        assert!(secret_file("certs/server.key"));
        assert!(!secret_file(".env.example"));
        assert!(!secret_file("src/main.rs"));
        // Experiment e6: the authentication area must not hide a secret file.
        assert!(secret_file("src/auth/secrets.json"));
        assert!(secret_file("config/credentials.yml"));
        assert!(secret_file(".npmrc"));
        assert!(secret_file("certs/client.p12"));
        assert!(!secret_file("src/auth/login.ts"));
        assert!(!secret_file("docs/secrets.md.example"));
    }

    #[test]
    fn removes_payment_and_platform_keys() {
        let text = "sk_live_51HpromptFAKE0123456789abcdefABCDEF rk_test_abcdefghij0123 whsec_abcdefghij0123456789 npm_abcdefghijklmnopqrstuvwxyz0123456789 glpat-abcdefghij0123456789 xoxe-1-abcdefghijkl";
        let (out, n) = redact(text);
        assert_eq!(n, 6, "{out}");
        assert_eq!(out.matches(MARK).count(), 6, "{out}");
    }

    #[test]
    fn removes_bearer_tokens_but_keeps_the_word() {
        let (out, n) = redact("curl -H \"Authorization: Bearer abcDEF0123456789.xyz\" https://x");
        assert_eq!(n, 1, "{out}");
        assert!(out.contains("Bearer [REMOVED BY AGENTPROOF]"), "{out}");
        // A Bearer key with a known format is removed once.
        let (out, n) = redact("Bearer sk_live_51HcmdFAKE0123456789abcdef");
        assert_eq!((out.as_str(), n), ("Bearer [REMOVED BY AGENTPROOF]", 1));
    }

    #[test]
    fn removes_secrets_from_each_text_of_a_json_value() {
        let mut v = serde_json::json!({
            "prompts": [{"text": "My key is sk_live_51HpromptFAKE0123456789abcdef"}],
            "nested": {"error": "invalid key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789"},
            "count": 3
        });
        json(&mut v);
        let s = v.to_string();
        assert!(!s.contains("sk_live") && !s.contains("sk-ant"), "{s}");
        assert_eq!(v["count"], 3);
    }

    #[test]
    fn removing_twice_changes_nothing() {
        let text =
            "postgres://app:s3cr3tpw@db Bearer abcDEF0123456789xyz api_key = \"abcdef123456\"";
        let once = redact(text).0;
        assert_eq!(redact(&once), (once.clone(), 0));
    }
}
