//! Whether an agent ended its turn by asking the owner something.
//!
//! To Claude Code a turn that ends "Reply yes and I'll overwrite the file" is a
//! turn that ended: `Stop` fires and the agent is idle. To the owner it is an
//! agent waiting on them, and one shown idle - off for a coffee, in the office -
//! is one they do not know to answer. `Stop` carries the closing message, so
//! Dex reads how it ends.
//!
//! The agent stays `idle`: answering it is an ordinary prompt, which a waiting
//! agent - one at a dialog, where typed text would answer the dialog - refuses.
//! What it asked goes in its status detail, and an idle agent has a detail for
//! no other reason. A guess, so a careful one: only how the message *ends*
//! counts, and the closing pleasantries every assistant uses are not questions.

/// How many closing sentences are read. A question is usually last, or followed
/// by one "otherwise..." sentence; one further back has been moved on from.
const CLOSING_SENTENCES: usize = 2;

/// The longest question worth showing: it sits on one line of a narrow panel.
const QUESTION_CHARS: usize = 120;

/// Ways of asking the owner for something without a question mark.
const CUES: [&str; 28] = [
    "reply \"",
    "reply with",
    "reply yes",
    "say yes",
    "say \"",
    "tell me and",
    "tell me if",
    "tell me which",
    "tell me whether",
    "tell me how",
    "tell me what",
    "let me know which",
    "let me know whether",
    "let me know if you want",
    "let me know if you'd like me",
    "let me know how you",
    "please confirm",
    "can you confirm",
    "could you confirm",
    "once you confirm",
    "if you confirm",
    "your approval",
    "your go-ahead",
    "your confirmation",
    "waiting for your",
    "need your",
    "need you to",
    "please choose",
];

/// Closings that ask for nothing.
const PLEASANTRIES: [&str; 6] = [
    "anything else",
    "let me know if you need",
    "let me know if you have",
    "feel free",
    "happy to help",
    "any questions",
];

/// An idle agent's status detail, from the input of the `Stop` that made it
/// idle: what it asked the owner, or `None`.
pub fn asked(stop_input: &serde_json::Value) -> Option<String> {
    let last = stop_input.get("last_assistant_message")?.as_str()?;
    question_for_owner(last).map(|question| format!("asked you: {question}"))
}

/// What the agent asked the owner as its turn ended, as one short line, or
/// `None` if the turn simply ended.
pub fn question_for_owner(last_message: &str) -> Option<String> {
    let sentences = sentences(last_message);
    // The last of the closing sentences that asks: what they are left looking at.
    sentences
        .iter()
        .rev()
        .take(CLOSING_SENTENCES)
        .find(|sentence| asks(sentence))
        .map(|sentence| one_line(sentence))
}

fn asks(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();
    if PLEASANTRIES.iter().any(|phrase| lower.contains(phrase)) {
        return false;
    }
    lower.trim_end().ends_with('?') || CUES.iter().any(|cue| lower.contains(cue))
}

/// The message as sentences: split after `.`, `!` or `?` where whitespace
/// follows (so not inside `essay-1.md`), and at blank lines.
fn sentences(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for paragraph in text.split("\n\n") {
        let mut current = String::new();
        let mut chars = paragraph.chars().peekable();
        while let Some(c) = chars.next() {
            current.push(c);
            let ends = matches!(c, '.' | '!' | '?')
                && chars.peek().is_none_or(|next| next.is_whitespace());
            if ends {
                found.push(std::mem::take(&mut current));
            }
        }
        found.push(current);
    }
    found
        .into_iter()
        .map(|sentence| sentence.trim().to_owned())
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

/// Markdown emphasis dropped, whitespace collapsed, cut to fit with an ellipsis.
fn one_line(sentence: &str) -> String {
    let plain: String = sentence
        .chars()
        .filter(|c| !matches!(c, '*' | '`'))
        .collect();
    let line = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= QUESTION_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(QUESTION_CHARS - 1).collect();
    cut.push('…');
    cut
}
