//! All text that Agentproof shows to users. The text follows ASD-STE100
//! Simplified Technical English: max 20 words in an instruction, max 25 words
//! in a description, one topic per sentence. The tests below check the length.

pub struct Summary<'a> {
    pub files: usize,
    pub added: u64,
    pub deleted: u64,
    pub commands: usize,
    pub failed: usize,
    pub before_session: usize,
    pub no_start: bool,
    pub no_git: bool,
    pub no_transcript: bool,
    pub data_path: &'a str,
}

pub fn summary(s: &Summary) -> String {
    let mut out = vec!["Agentproof examined this session.".to_string()];
    if s.no_git {
        out.push(
            "This folder is not a Git repository. Thus Agentproof cannot examine the changes."
                .into(),
        );
    } else if s.files == 0 {
        out.push("The agent did not change files.".into());
    } else {
        out.push(format!(
            "The agent changed {}: {} added and {} deleted.",
            count(s.files, "file", "files"),
            count(s.added as usize, "line", "lines"),
            count(s.deleted as usize, "line", "lines"),
        ));
    }
    if s.no_transcript {
        out.push("Agentproof cannot read the session transcript.".into());
    } else if s.commands == 0 {
        out.push("The agent did not run commands.".into());
    } else {
        out.push(format!(
            "The agent ran {}.",
            count(s.commands, "command", "commands")
        ));
        if s.failed > 0 {
            out.push(format!("{} of the commands failed.", s.failed));
        }
    }
    if s.before_session > 0 {
        out.push(format!(
            "{} had changes before the session. Agentproof does not count these changes as work of the agent.",
            count(s.before_session, "file", "files"),
        ));
    }
    if s.no_start && !s.no_git {
        out.push(
            "Agentproof did not find the start point of this session. Thus it compared the changes with the last commit only."
                .into(),
        );
    }
    out.push("This version has no rule checks. Thus this message is not a verdict.".into());
    out.push(format!("The full data is in {}.", s.data_path));
    out.join(" ")
}

pub fn engine_error(reason: &str) -> String {
    format!("Agentproof cannot examine this session. The reason is: {reason}")
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_WORDS: usize = 25;

    fn assert_ste(text: &str) {
        for sentence in text.split(". ").map(|s| s.trim_end_matches('.')) {
            let words = sentence.split_whitespace().count();
            assert!(
                words <= MAX_WORDS,
                "{words} words (max {MAX_WORDS}): {sentence}"
            );
        }
    }

    fn sample() -> Summary<'static> {
        Summary {
            files: 1,
            added: 1,
            deleted: 0,
            commands: 3,
            failed: 1,
            before_session: 2,
            no_start: true,
            no_git: false,
            no_transcript: false,
            data_path: "C:/Users/x/.agentproof/p/s/check.json",
        }
    }

    #[test]
    fn summary_follows_ste100_length_limits() {
        assert_ste(&summary(&sample()));
        assert_ste(&summary(&Summary {
            no_git: true,
            no_transcript: true,
            ..sample()
        }));
        assert_ste(&summary(&Summary {
            files: 0,
            commands: 0,
            before_session: 0,
            ..sample()
        }));
        assert_ste(&engine_error("git diff failed"));
    }

    #[test]
    fn summary_uses_singular_and_plural() {
        let text = summary(&sample());
        assert!(
            text.contains("changed 1 file: 1 line added and 0 lines deleted."),
            "{text}"
        );
        assert!(
            text.contains("ran 3 commands. 1 of the commands failed."),
            "{text}"
        );
    }

    #[test]
    fn summary_never_claims_a_verdict() {
        assert!(summary(&sample()).contains("not a verdict"));
    }
}
