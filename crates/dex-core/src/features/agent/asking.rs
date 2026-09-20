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
//! What it asked goes in its status detail, as `asked you: ...`. A guess, so a
//! careful one, and one with a net under it: an idle agent that did not ask
//! anything still says how its turn ended (`said: ...`), so the owner can read
//! a closing line the guess got wrong. The guess: only how the message *ends*
//! counts, and the closing pleasantries every assistant uses are not questions.
//! A question need not end in a question mark: the owner's first test of this
//! was "ask me something, but end it with a period".

/// How many closing sentences are read. A question is usually last, or followed
/// by one "otherwise..." sentence; one further back has been moved on from.
const CLOSING_SENTENCES: usize = 2;

/// The longest question worth showing: it sits on one line of a narrow panel.
const QUESTION_CHARS: usize = 120;

/// Ways of asking the owner for something without a question mark.
const CUES: [&str; 36] = [
    "reply \"",
    "reply with \"",
    "reply with yes",
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
    // From real closing messages the first list missed.
    "say the word",
    "say go",
    "say if you",
    "tell me first",
    "tell me when",
    "your call",
    "send me the",
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

const ASKED: &str = "asked you: ";
const SAID: &str = "said: ";

/// An idle agent's status detail, from the input of the `Stop` that made it
/// idle: what it asked the owner, or else what it said last - the guess about
/// questions can miss, and the owner can read a closing line for themselves.
pub fn idle_detail(stop_input: &serde_json::Value) -> Option<String> {
    let last = stop_input.get("last_assistant_message")?.as_str()?;
    if let Some(question) = question_for_owner(last) {
        return Some(format!("{ASKED}{question}"));
    }
    let closing = last.trim().rsplit("\n\n").next().map(one_line)?;
    (!closing.is_empty()).then(|| format!("{SAID}{closing}"))
}

/// Whether this status detail is a question the agent left the owner: its
/// pane is waiting for an answer, so nothing else may be typed there.
pub fn asked_the_owner(detail: Option<&str>) -> bool {
    detail.is_some_and(|detail| detail.starts_with(ASKED))
}

/// Whether a detail stays out of the workspace log. The log is the activity
/// feed, and every finished turn has last words: written there they would bury
/// what happened under what was said, and the owner already sees them on the
/// agent. (Other agents' digests skip status events in any case.)
pub fn stays_out_of_the_log(detail: &str) -> bool {
    detail.starts_with(SAID)
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
    lower.trim_end().ends_with('?') || has_cue(&lower) || is_inverted(&lower)
}

/// Whether one of `CUES` is in the sentence as whole words: "say go" is not in
/// "say good things", nor "your call" in "your callback". A cue that ends in a
/// quotation mark is followed by whatever is being quoted.
fn has_cue(lower: &str) -> bool {
    let spaced: String = lower
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '\'' | '"') {
                c
            } else {
                ' '
            }
        })
        .collect();
    let words = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let padded = format!(" {words} ");
    CUES.iter().any(|cue| {
        let end = if cue.ends_with('"') { "" } else { " " };
        padded.contains(&format!(" {cue}{end}"))
    })
}

const AUXILIARIES: [&str; 17] = [
    "do", "does", "did", "is", "are", "was", "were", "can", "could", "would", "will", "should",
    "shall", "have", "has", "may", "might",
];
const SUBJECTS: [&str; 8] = ["you", "i", "we", "it", "they", "this", "that", "your"];
const QUESTION_WORDS: [&str; 9] = [
    "what", "which", "who", "whom", "whose", "where", "when", "why", "how",
];
/// How far after a question word its verb may come: "which of the two do you".
const REACH: usize = 5;

/// Whether the sentence is built as a question, whatever it ends with: English
/// asks by putting the verb before its subject. "Would you like..." opens with
/// one; "what would it be" has one after its question word, where the statement
/// "what it would be" does not.
fn is_inverted(lower: &str) -> bool {
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .collect();
    let inverted_at = |at: usize| {
        words.get(at).is_some_and(|word| AUXILIARIES.contains(word))
            && words
                .get(at + 1)
                .is_some_and(|word| SUBJECTS.contains(word))
    };
    if inverted_at(0) || (words.first() == Some(&"is") && words.get(1) == Some(&"there")) {
        return true;
    }
    words
        .iter()
        .enumerate()
        .any(|(at, word)| QUESTION_WORDS.contains(word) && (at + 1..=at + REACH).any(inverted_at))
}

/// A line with its list marker taken off, if it has one: `- `, `* `, `1. `, `2) `.
/// Only a marker: "2 or 3 replicas?" opens with a number that is part of it.
fn list_item(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if let Some(rest) = ["- ", "* ", "\u{2022} "]
        .iter()
        .find_map(|mark| line.strip_prefix(mark))
    {
        return Some(rest);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let rest = &line[digits..];
    let rest = rest
        .strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))?;
    (digits > 0).then_some(rest)
}

/// The message as the separate things it says. A blank line, a list item, and
/// the end of a sentence each start a new one; a line end alone does not, or a
/// question an agent hard-wrapped would be shown as its last three words. Then
/// each is split after `.`, `!` or `?` where whitespace follows (so not inside
/// `essay-1.md`). A closing list is so many things said, not one long sentence
/// to find a cue in.
fn sentences(text: &str) -> Vec<String> {
    let mut blocks: Vec<(String, bool)> = Vec::new();
    for line in text.lines() {
        let item = list_item(line);
        let said = item.unwrap_or(line).trim();
        if said.is_empty() {
            blocks.push((String::new(), false));
            continue;
        }
        let continues = blocks.last().is_some_and(|(before, was_item)| {
            let indented = line.starts_with([' ', '\t']);
            let open = !before.is_empty() && !before.ends_with(['.', '!', '?', ':']);
            item.is_none() && open && (!was_item || indented)
        });
        match blocks.last_mut() {
            Some((before, _)) if continues => {
                before.push(' ');
                before.push_str(said);
            }
            _ => blocks.push((said.to_owned(), item.is_some())),
        }
    }
    let mut found = Vec::new();
    for (block, _) in &blocks {
        let mut current = String::new();
        let mut chars = block.chars().peekable();
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
