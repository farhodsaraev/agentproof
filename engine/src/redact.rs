//! Remove secrets from all text before it goes to an AI provider.

use std::sync::OnceLock;

use regex::Regex;

pub const MARK: &str = "[REMOVED BY AGENTPROOF]";

struct Patterns {
    /// Secrets that are removed completely.
    whole: Vec<Regex>,
    /// `name = value` pairs: the name stays, the value goes.
    assignment: Regex,
    /// `scheme://user:password@host`: the password goes.
    url_password: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        whole: [
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(-----END [A-Z ]*PRIVATE KEY-----|$)",
            r"sk-ant-[A-Za-z0-9_\-]{20,}",
            r"sk-(?:proj-)?[A-Za-z0-9_\-]{20,}",
            r"gh[pousr]_[A-Za-z0-9]{30,}",
            r"github_pat_[A-Za-z0-9_]{30,}",
            r"AKIA[0-9A-Z]{16}",
            r"xox[abprs]-[A-Za-z0-9\-]{10,}",
            r"AIza[0-9A-Za-z_\-]{35}",
            r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        ]
        .iter()
        .map(|p| Regex::new(p).expect("valid pattern"))
        .collect(),
        assignment: Regex::new(
            // `NAME=value`, `name: value` and `name = "value"`. A value without
            // quotes after ` = ` is code (`x = form.get(...)`), not a secret.
            r#"(?i)((?:api[_-]?key|secret|token|passw(?:or)?d|pwd|credential|auth[_-]?key|private[_-]?key)[A-Za-z0-9_\-]*["']?(?:\s*:\s*["']?|=["']?|\s*=\s*["']))([^\s"',;()]{6,})"#,
        )
        .expect("valid pattern"),
        url_password: Regex::new(r"([a-z][a-z0-9+.\-]*://[^/\s:@]+:)([^@\s/]+)(@)").expect("valid pattern"),
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
    (out, count)
}

/// True for files whose whole content is secret, for example `.env` or a private key.
pub fn secret_file(path: &str) -> bool {
    matches!(
        crate::checks::sensitive_area(path),
        Some(crate::checks::Area::Secrets)
    ) && !path.to_lowercase().ends_with(".example")
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
    }
}
