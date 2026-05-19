use std::{
    collections::{HashMap, HashSet},
    process::Command,
};

use crate::{commands::apply_redirects, pipe::NyPipe};

pub struct NyAlias {
    alias: HashMap<String, String>,
}

impl Default for NyAlias {
    fn default() -> Self {
        Self::new()
    }
}

impl NyAlias {
    pub fn new() -> Self {
        Self {
            alias: HashMap::new(),
        }
    }

    pub fn parse_input(&mut self, input: Vec<String>) -> Result<(), String> {
        match input.get(1) {
            Some(part) => match part.as_str() {
                "--list" => self.list(),

                "--set" => if let Some(err) = self.insert(input) { return Err(err) },

                _ => return Err("Unknown arg".to_string()),
            },
            None => return Err("Missing args! Usage: alias --set <alias> <commands>".to_string()),
        }

        Ok(())
    }

    pub fn run_alias(&self, input: Vec<String>) -> Result<i32, String> {
        let parts = self
            .resolve_alias(&input)?
            .ok_or("Alias not found".to_string())?;

        if let Some(pipe) = NyPipe::new(&parts)? {
            return pipe.run();
        }

        run_parts(&parts)
    }

    pub fn resolve_alias(&self, input: &[String]) -> Result<Option<Vec<String>>, String> {
        let Some(first) = input.first() else {
            return Ok(None);
        };

        if !self.alias.contains_key(first) {
            return Ok(None);
        }

        let mut seen = HashSet::new();
        let mut parts = input.to_vec();

        loop {
            let Some(name) = parts.first() else {
                return Ok(None);
            };

            let Some(alias) = self.alias.get(name) else {
                break;
            };

            if !seen.insert(name.clone()) {
                return Err(format!("Alias loop detected for '{name}'"));
            }

            let mut expanded = shell_words::split(alias)
                .map_err(|err| format!("Failed to parse alias '{name}': {err}"))?;
            expanded.extend_from_slice(&parts[1..]);
            parts = expanded;
        }

        Ok(Some(parts))
    }

    fn list(&self) {
        for (alias, command) in &self.alias {
            println!("{alias} => {command}")
        }
    }

    fn insert(&mut self, args: Vec<String>) -> Option<String> {
        if args.len() <= 3 {
            return Some("Missing args! Usage: alias --set <alias> <commands>".to_string());
        }

        if let Some(key) = args.get(2) {
            let cmd = args[3..].join(" ");
            self.alias.insert(key.clone(), cmd);

            return None;
        }

        Some("Unknown Error! Usage: alias --set <alias> <commands>".to_string())
    }
}

fn run_parts(parts: &[String]) -> Result<i32, String> {
    let program = parts.first().ok_or("missing command".to_string())?;
    let mut cmd = Command::new(program);

    let redirected = apply_redirects(&mut cmd, &parts[1..])?;
    cmd.args(&redirected.args);

    let status = cmd
        .spawn()
        .map_err(|err| format!("{program}: {err}"))?
        .wait()
        .map_err(|err| format!("{program}: {err}"))?;

    Ok(status.code().unwrap_or(130))
}
