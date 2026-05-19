#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Always,
    OnSuccess,
    OnFailure,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChainPart {
    pub mode: RunMode,
    pub parts: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    MissingClosingQuote,
    EmptyCommand,
    UnsupportedOperator,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingClosingQuote => f.write_str("missing closing quote"),
            ParseError::EmptyCommand => f.write_str("empty command around operator"),
            ParseError::UnsupportedOperator => {
                f.write_str("unsupported operator, background jobs are not implemented")
            }
        }
    }
}

impl std::error::Error for ParseError {}

enum State {
    Delimiter,
    Backslash,
    Unquoted,
    UnquotedBackslash,
    SingleQuoted,
    DoubleQuoted,
    DoubleQuotedBackslash,
    Comment,
}

enum Token {
    Word(String),
    Operator(String),
}

pub fn parse_line(s: &str) -> Result<Vec<ChainPart>, ParseError> {
    let tokens = tokenize(s)?;
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut mode = RunMode::Always;

    for token in tokens {
        match token {
            Token::Operator(op) if matches!(op.as_str(), "&&" | "||" | ";") => {
                if current.is_empty() {
                    return Err(ParseError::EmptyCommand);
                }

                out.push(ChainPart {
                    mode,
                    parts: std::mem::take(&mut current),
                });

                mode = match op.as_str() {
                    "&&" => RunMode::OnSuccess,
                    "||" => RunMode::OnFailure,
                    ";" => RunMode::Always,
                    _ => RunMode::Always,
                };
            }

            Token::Operator(op) if op == "&" => return Err(ParseError::UnsupportedOperator),

            Token::Word(word) => current.push(word),
            Token::Operator(op) => current.push(op),
        }
    }

    if current.is_empty() {
        if out.is_empty() {
            return Ok(Vec::new());
        }

        return Err(ParseError::EmptyCommand);
    }

    out.push(ChainPart {
        mode,
        parts: current,
    });

    Ok(out)
}

fn tokenize(s: &str) -> Result<Vec<Token>, ParseError> {
    use State::*;

    let mut words = Vec::new();
    let mut word = String::new();
    let mut chars = s.chars().peekable();
    let mut state = Delimiter;

    loop {
        let c = chars.next();
        state = match state {
            Delimiter => match c {
                None => break,
                Some('\'') => SingleQuoted,
                Some('"') => DoubleQuoted,
                Some('\\') => Backslash,
                Some('\t') | Some(' ') | Some('\n') => Delimiter,
                Some('#') => Comment,
                Some(c @ ';') | Some(c @ '<') | Some(c @ '>') | Some(c @ '|') | Some(c @ '&') => {
                    words.push(Token::Operator(read_operator(c, &mut chars)));
                    Delimiter
                }
                Some(c) => {
                    word.push(c);
                    Unquoted
                }
            },

            Backslash => match c {
                None => {
                    word.push('\\');
                    words.push(Token::Word(std::mem::take(&mut word)));
                    break;
                }
                Some('\n') => Delimiter,
                Some(c) => {
                    word.push(c);
                    Unquoted
                }
            },

            Unquoted => match c {
                None => {
                    words.push(Token::Word(std::mem::take(&mut word)));
                    break;
                }
                Some('\'') => SingleQuoted,
                Some('"') => DoubleQuoted,
                Some('\\') => UnquotedBackslash,
                Some('\t') | Some(' ') | Some('\n') => {
                    words.push(Token::Word(std::mem::take(&mut word)));
                    Delimiter
                }
                Some(c @ ';') | Some(c @ '<') | Some(c @ '>') | Some(c @ '|') | Some(c @ '&') => {
                    words.push(Token::Word(std::mem::take(&mut word)));
                    words.push(Token::Operator(read_operator(c, &mut chars)));
                    Delimiter
                }
                Some(c) => {
                    word.push(c);
                    Unquoted
                }
            },

            UnquotedBackslash => match c {
                None => {
                    word.push('\\');
                    words.push(Token::Word(std::mem::take(&mut word)));
                    break;
                }
                Some('\n') => Unquoted,
                Some(c) => {
                    word.push(c);
                    Unquoted
                }
            },

            SingleQuoted => match c {
                None => return Err(ParseError::MissingClosingQuote),
                Some('\'') => Unquoted,
                Some(c) => {
                    word.push(c);
                    SingleQuoted
                }
            },

            DoubleQuoted => match c {
                None => return Err(ParseError::MissingClosingQuote),
                Some('"') => Unquoted,
                Some('\\') => DoubleQuotedBackslash,
                Some(c) => {
                    word.push(c);
                    DoubleQuoted
                }
            },

            DoubleQuotedBackslash => match c {
                None => return Err(ParseError::MissingClosingQuote),
                Some('\n') => DoubleQuoted,
                Some(c @ '$') | Some(c @ '`') | Some(c @ '"') | Some(c @ '\\') => {
                    word.push(c);
                    DoubleQuoted
                }
                Some(c) => {
                    word.push('\\');
                    word.push(c);
                    DoubleQuoted
                }
            },

            Comment => match c {
                None => break,
                Some('\n') => Delimiter,
                Some(_) => Comment,
            },
        };
    }

    Ok(words)
}

fn read_operator(first: char, chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    match first {
        '&' if chars.peek() == Some(&'&') => {
            chars.next();
            "&&".to_string()
        }
        '|' if chars.peek() == Some(&'|') => {
            chars.next();
            "||".to_string()
        }
        '>' if chars.peek() == Some(&'>') => {
            chars.next();
            ">>".to_string()
        }
        _ => first.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ChainPart, ParseError, RunMode, parse_line};

    #[test]
    fn parses_chain_operators_without_spaces() {
        assert_eq!(
            parse_line("echo hi&&pwd;ls||echo nope").unwrap(),
            vec![
                ChainPart {
                    mode: RunMode::Always,
                    parts: vec!["echo".to_string(), "hi".to_string()],
                },
                ChainPart {
                    mode: RunMode::OnSuccess,
                    parts: vec!["pwd".to_string()],
                },
                ChainPart {
                    mode: RunMode::Always,
                    parts: vec!["ls".to_string()],
                },
                ChainPart {
                    mode: RunMode::OnFailure,
                    parts: vec!["echo".to_string(), "nope".to_string()],
                },
            ]
        );
    }

    #[test]
    fn keeps_quoted_operators_literal() {
        assert_eq!(
            parse_line(r#"echo "a && b" ';'"#).unwrap(),
            vec![ChainPart {
                mode: RunMode::Always,
                parts: vec!["echo".to_string(), "a && b".to_string(), ";".to_string(),],
            }]
        );
    }

    #[test]
    fn rejects_empty_commands() {
        assert_eq!(
            parse_line("echo hi &&").unwrap_err(),
            ParseError::EmptyCommand
        );
    }

    #[test]
    fn rejects_background_operator() {
        assert_eq!(
            parse_line("sleep 1 &").unwrap_err(),
            ParseError::UnsupportedOperator
        );
    }
}
