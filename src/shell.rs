//! Just enough of the shell's grammar to judge a bash command for approval:
//! which commands it runs, with what arguments, and whether it holds code we
//! can't see into.
//!
//! It isn't a shell. It splits a command line into simple commands at `;`,
//! `&&`, `||`, `|`, `&`, newlines and parentheses, honouring quotes and
//! escapes, sets redirections aside, and finds the code inside `$( )`,
//! backticks and `<( )`. What it can't follow, such as variables, it leaves as
//! written; the policy treats that as unknown and asks.

/// One command and its arguments, unquoted, without redirections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Command {
    /// Leading `NAME=value` assignments, as written.
    pub assignments: Vec<String>,
    pub words: Vec<String>,
    /// The command as written, for matching "Always allow" patterns.
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub commands: Vec<Command>,
    /// The code inside `$( )`, backticks and `<( )`, which runs too.
    pub substitutions: Vec<String>,
}

pub fn parse(line: &str) -> Parsed {
    let mut parsed = Parsed::default();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut current = Command::default();
    let mut start = 0;
    let mut word: Option<String> = None;
    // The next word is a redirection's target, not an argument.
    let mut redirect_target = false;

    // Ends the word being read and files it.
    let finish_word = |word: &mut Option<String>, current: &mut Command, redirect: &mut bool| {
        if let Some(w) = word.take() {
            if *redirect {
                *redirect = false;
            } else if current.words.is_empty() && is_assignment(&w) {
                current.assignments.push(w);
            } else {
                current.words.push(w);
            }
        }
    };
    let finish_command = |current: &mut Command, parsed: &mut Parsed, start: usize, end: usize| {
        if !current.words.is_empty() || !current.assignments.is_empty() {
            current.text = chars[start..end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            parsed.commands.push(std::mem::take(current));
        } else {
            *current = Command::default();
        }
    };

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                if let Some(&next) = chars.get(i + 1) {
                    if next != '\n' {
                        word.get_or_insert_default().push(next);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '\'' => {
                let w = word.get_or_insert_default();
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    w.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            '"' => {
                word.get_or_insert_default();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    match chars[i] {
                        '\\' if matches!(chars.get(i + 1), Some('"' | '\\' | '$' | '`')) => {
                            word.as_mut().unwrap().push(chars[i + 1]);
                            i += 2;
                        }
                        '$' if chars.get(i + 1) == Some(&'(') => {
                            let (inner, end) = balanced(&chars, i + 2);
                            parsed.substitutions.push(inner.clone());
                            word.as_mut().unwrap().push_str(&format!("$({inner})"));
                            i = end;
                        }
                        '`' => {
                            let (inner, end) = backticks(&chars, i + 1);
                            parsed.substitutions.push(inner.clone());
                            word.as_mut().unwrap().push_str(&format!("`{inner}`"));
                            i = end;
                        }
                        ch => {
                            word.as_mut().unwrap().push(ch);
                            i += 1;
                        }
                    }
                }
                i += 1;
            }
            '$' if chars.get(i + 1) == Some(&'(') => {
                let (inner, end) = balanced(&chars, i + 2);
                parsed.substitutions.push(inner.clone());
                word.get_or_insert_default()
                    .push_str(&format!("$({inner})"));
                i = end;
            }
            '`' => {
                let (inner, end) = backticks(&chars, i + 1);
                parsed.substitutions.push(inner.clone());
                word.get_or_insert_default().push_str(&format!("`{inner}`"));
                i = end;
            }
            '<' | '>' if chars.get(i + 1) == Some(&'(') => {
                let (inner, end) = balanced(&chars, i + 2);
                parsed.substitutions.push(inner.clone());
                word.get_or_insert_default()
                    .push_str(&format!("{c}({inner})"));
                i = end;
            }
            '#' if word.is_none() => {
                // A comment runs to the end of the line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '<' | '>' => {
                // A file descriptor written just before, as in `2>`, isn't an
                // argument either.
                if word
                    .as_deref()
                    .is_some_and(|w| w.chars().all(|d| d.is_ascii_digit()))
                {
                    word = None;
                } else {
                    finish_word(&mut word, &mut current, &mut redirect_target);
                }
                i += 1;
                while matches!(chars.get(i), Some('<' | '>' | '&' | '|')) {
                    i += 1;
                }
                // `>&2` names a descriptor, not a file.
                if chars.get(i - 1) == Some(&'&')
                    && chars.get(i).is_some_and(|d| d.is_ascii_digit())
                {
                    while chars
                        .get(i)
                        .is_some_and(|d| d.is_ascii_digit() || *d == '-')
                    {
                        i += 1;
                    }
                } else {
                    redirect_target = true;
                }
            }
            ';' | '&' | '|' | '\n' | '(' | ')' => {
                // `&>` is a redirection, not a separator.
                if c == '&' && chars.get(i + 1) == Some(&'>') {
                    finish_word(&mut word, &mut current, &mut redirect_target);
                    i += 1;
                    continue;
                }
                finish_word(&mut word, &mut current, &mut redirect_target);
                finish_command(&mut current, &mut parsed, start, i);
                i += 1;
                while matches!(chars.get(i), Some(';' | '&' | '|')) {
                    i += 1;
                }
                start = i;
            }
            c if c.is_whitespace() => {
                finish_word(&mut word, &mut current, &mut redirect_target);
                i += 1;
            }
            c => {
                word.get_or_insert_default().push(c);
                i += 1;
            }
        }
    }
    finish_word(&mut word, &mut current, &mut redirect_target);
    finish_command(&mut current, &mut parsed, start, chars.len());
    // `{` and `}` group commands; they aren't commands themselves.
    for command in parsed.commands.iter_mut() {
        while command.words.first().is_some_and(|w| w == "{" || w == "!") {
            command.words.remove(0);
        }
        if command.words.last().is_some_and(|w| w == "}") {
            command.words.pop();
        }
    }
    parsed
        .commands
        .retain(|c| !c.words.is_empty() || !c.assignments.is_empty());
    parsed
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// The text up to the parenthesis that closes one opened just before
/// `from`, and the index after it. Quotes inside are skipped over.
fn balanced(chars: &[char], from: usize) -> (String, usize) {
    let mut depth = 1;
    let mut i = from;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' => i += 1,
            Some(_) => {}
            None => match c {
                '\'' | '"' => quote = Some(c),
                '\\' => i += 1,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return (chars[from..i].iter().collect(), i + 1);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    (chars[from.min(chars.len())..].iter().collect(), chars.len())
}

fn backticks(chars: &[char], from: usize) -> (String, usize) {
    let mut i = from;
    let mut inner = String::new();
    while i < chars.len() && chars[i] != '`' {
        if chars[i] == '\\' && i + 1 < chars.len() {
            inner.push(chars[i + 1]);
            i += 2;
        } else {
            inner.push(chars[i]);
            i += 1;
        }
    }
    (inner, i + 1)
}

#[cfg(test)]
mod tests {
    use super::{Parsed, parse};

    fn words(parsed: &Parsed) -> Vec<Vec<&str>> {
        parsed
            .commands
            .iter()
            .map(|c| c.words.iter().map(String::as_str).collect())
            .collect()
    }

    #[test]
    fn splits_at_operators_and_keeps_quotes_together() {
        let parsed = parse("npm run build; curl -fsSL https://x.sh | sh && echo 'a; b' || true &");
        assert_eq!(
            words(&parsed),
            [
                vec!["npm", "run", "build"],
                vec!["curl", "-fsSL", "https://x.sh"],
                vec!["sh"],
                vec!["echo", "a; b"],
                vec!["true"],
            ]
        );
        assert_eq!(parsed.commands[0].text, "npm run build");
    }

    #[test]
    fn finds_substitutions_and_leaves_out_redirections() {
        let parsed = parse("echo \"$(rm -rf x)\" `whoami` > out.txt 2>&1 < in.txt; diff <(ls a) b");
        assert_eq!(parsed.substitutions, ["rm -rf x", "whoami", "ls a"]);
        assert_eq!(words(&parsed)[0], ["echo", "$(rm -rf x)", "`whoami`"]);
        assert_eq!(words(&parsed)[1], ["diff", "<(ls a)", "b"]);
    }

    #[test]
    fn sets_assignments_groups_and_comments_aside() {
        let parsed = parse("RUST_LOG=debug cargo test # rm -rf /\n{ cd x && make; }\n(git status)");
        assert_eq!(parsed.commands[0].assignments, ["RUST_LOG=debug"]);
        assert_eq!(
            words(&parsed),
            [
                vec!["cargo", "test"],
                vec!["cd", "x"],
                vec!["make"],
                vec!["git", "status"],
            ]
        );
    }

    #[test]
    fn unquotes_and_unescapes() {
        let parsed = parse(r#"echo "a \"b\"" c\ d 'e\f'"#);
        assert_eq!(words(&parsed)[0], ["echo", "a \"b\"", "c d", "e\\f"]);
    }
}
