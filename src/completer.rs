use std::{cmp::Ordering, path::PathBuf};

use reedline::{Completer, Suggestion};
use strsim::jaro_winkler;

use crate::commands::Cmd;

pub struct NyCompleter {
    pub commands: Vec<Cmd>,
}

impl Completer for NyCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let before = &line[..pos];
        let span_start = current_token_start(before);
        let query = &before[span_start..];

        if before.starts_with("cd ") {
            return complete_path(query, span_start, pos, true);
        }

        if span_start == 0 {
            return complete_command(&self.commands, query, pos);
        }

        complete_path(query, span_start, pos, false)
    }
}

fn complete_command(commands: &[Cmd], query: &str, pos: usize) -> Vec<Suggestion> {
    let mut matches: Vec<(String, f64)> = commands
        .iter()
        .filter_map(|cmd| match_score(query, &cmd.name).map(|score| (cmd.name.clone(), score)))
        .collect();

    sort_matches(&mut matches);

    matches
        .into_iter()
        .map(|(name, _)| Suggestion {
            value: name,
            description: None,
            extra: None,
            span: reedline::Span { start: 0, end: pos },
            append_whitespace: true,
            style: None,
            display_override: None,
            match_indices: None,
        })
        .collect()
}

fn complete_path(query: &str, span_start: usize, pos: usize, dirs_only: bool) -> Vec<Suggestion> {
    let (dir_part, name_part) = match query.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name),
        None => ("".to_string(), query),
    };

    let read_dir_path = if dir_part.is_empty() {
        PathBuf::from(".")
    } else {
        expand_path(&dir_part)
    };

    let mut matches: Vec<(Suggestion, f64)> = std::fs::read_dir(read_dir_path)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_dir = path.is_dir();

            if dirs_only && !is_dir {
                return None;
            }

            let name = path.file_name()?.to_string_lossy().to_string();
            let score = match_score(name_part, &name)?;

            let value = if is_dir {
                format!("{dir_part}{name}/")
            } else {
                format!("{dir_part}{name}")
            };

            let display = if is_dir {
                format!("{name}/")
            } else {
                name.clone()
            };

            Some((
                Suggestion {
                    value,
                    description: None,
                    extra: None,
                    span: reedline::Span {
                        start: span_start,
                        end: pos,
                    },
                    append_whitespace: !is_dir,
                    style: None,
                    display_override: Some(display),
                    match_indices: None,
                },
                score,
            ))
        })
        .collect();

    matches.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.0.value.cmp(&b.0.value))
    });

    matches
        .into_iter()
        .map(|(suggestion, _)| suggestion)
        .collect()
}

fn current_token_start(line: &str) -> usize {
    line.rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0)
}

fn match_score(query: &str, candidate: &str) -> Option<f64> {
    if query.is_empty() {
        return Some(1.0);
    }

    if candidate.starts_with(query) {
        return Some(3.0);
    }

    if candidate.contains(query) {
        return Some(2.0);
    }

    let score = jaro_winkler(query, candidate);
    if score >= 0.84 {
        return Some(score);
    }

    None
}

fn sort_matches(matches: &mut [(String, f64)]) {
    matches.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
    } else if path.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(path.replacen('~', &home, 1))
    } else {
        PathBuf::from(path)
    }
}
