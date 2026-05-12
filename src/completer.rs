use std::path::PathBuf;

use reedline::{Completer, Suggestion};

use crate::commands::Cmd;

pub struct NyCompleter {
    pub commands: Vec<Cmd>,
}

impl Completer for NyCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let before = &line[..pos];

        if before.starts_with("cd ") {
            return complete_cd(line, pos);
        }

        if before.contains(' ') {
            return vec![];
        }

        self.commands
            .iter()
            .filter(|cmd| cmd.name.starts_with(before))
            .map(|cmd| Suggestion {
                value: cmd.name.clone(),
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
}

fn complete_cd(line: &str, pos: usize) -> Vec<Suggestion> {
    let before = &line[..pos];

    let query = before.strip_prefix("cd ").unwrap_or("");

    let (dir_part, name_part) = match query.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name),
        None => ("".to_string(), query),
    };

    let read_dir_path = if dir_part.is_empty() {
        PathBuf::from(".")
    } else {
        expand_path(&dir_part)
    };

    std::fs::read_dir(read_dir_path)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();

            if !path.is_dir() {
                return None;
            }

            let name = path.file_name()?.to_string_lossy().to_string();

            if !name.starts_with(name_part) {
                return None;
            }

            Some(Suggestion {
                value: format!("{dir_part}{name}"),
                description: None,
                extra: None,
                span: reedline::Span { start: 3, end: pos },
                append_whitespace: false,
                style: None,
                display_override: Some(name.clone()),
                match_indices: None,
            })
        })
        .collect()
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        PathBuf::from(std::env::var("HOME").unwrap())
    } else if path.starts_with("~/") {
        let home = std::env::var("HOME").unwrap();
        PathBuf::from(path.replacen('~', &home, 1))
    } else {
        PathBuf::from(path)
    }
}
