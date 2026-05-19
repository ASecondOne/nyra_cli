use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

use crate::vars::Vars;

#[derive(Clone)]
pub struct Cmd {
    pub name: String,
    pub path: PathBuf,
}

pub struct NyCommand {
    cmds: Vec<Cmd>,
}

pub struct RedirectedArgs {
    pub args: Vec<String>,
    pub stdout_redirected: bool,
}

impl NyCommand {
    pub fn new() -> Self {
        NyCommand { cmds: Vec::new() }
    }

    pub fn load_commands(&mut self) {
        self.cmds = std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten())
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = path.file_name()?.to_string_lossy().to_string();
                Some(Cmd { name, path })
            })
            .collect()
    }

    pub fn get_commands(&self) -> &[Cmd] {
        &self.cmds
    }
}

impl Default for NyCommand {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run_command(
    command: Cmd,
    args: &[&str],
    env_vars: &Vars,
    current_pid: Arc<Mutex<Option<u32>>>,
) -> Result<i32, String> {
    let expanded_args: Vec<String> = args.iter().map(|arg| env_vars.expand_vars(arg)).collect();

    let mut cmd = Command::new(command.path);

    for (k, v) in env_vars.get_vars() {
        cmd.env(k, v);
    }

    let redirected = apply_redirects(&mut cmd, &expanded_args)?;
    cmd.args(&redirected.args);

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("{}: {err}", command.name))?;
    if let Ok(mut pid) = current_pid.lock() {
        *pid = Some(child.id());
    }

    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => {
            if let Ok(mut pid) = current_pid.lock() {
                *pid = None;
            }
            return Err(format!("{}: {err}", command.name));
        }
    };

    if let Ok(mut pid) = current_pid.lock() {
        *pid = None;
    }

    Ok(status.code().unwrap_or(130))
}

pub fn apply_redirects(cmd: &mut Command, parts: &[String]) -> Result<RedirectedArgs, String> {
    let mut args = Vec::new();
    let mut i = 0;
    let mut stdout_redirected = false;

    while i < parts.len() {
        match parts[i].as_str() {
            "<" => {
                let file = parts
                    .get(i + 1)
                    .ok_or("missing input file after '<'".to_string())?;
                let file = File::open(file)
                    .map_err(|err| format!("failed to open '{file}' for reading: {err}"))?;
                cmd.stdin(Stdio::from(file));
                i += 2;
            }

            ">" | ">>" => {
                let file = parts
                    .get(i + 1)
                    .ok_or(format!("missing output file after '{}'", parts[i]))?;
                let file = if parts[i] == ">>" {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file)
                        .map_err(|err| format!("failed to open '{file}' for appending: {err}"))?
                } else {
                    File::create(file).map_err(|err| format!("failed to create '{file}': {err}"))?
                };

                cmd.stdout(Stdio::from(file));
                stdout_redirected = true;
                i += 2;
            }

            _ => {
                args.push(parts[i].clone());
                i += 1;
            }
        }
    }

    Ok(RedirectedArgs {
        args,
        stdout_redirected,
    })
}
