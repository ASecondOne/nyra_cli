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
) -> Option<i32> {
    let expanded_args: Vec<String> = args.iter().map(|arg| env_vars.expand_vars(arg)).collect();

    let expanded_refs: Vec<&str> = expanded_args.iter().map(String::as_str).collect();

    let mut cmd = Command::new(command.path);

    for (k, v) in env_vars.get_vars() {
        cmd.env(k, v);
    }

    if let Some(pos) = expanded_refs.iter().position(|&r| r == ">" || r == ">>") {
        let real_args = &expanded_refs[..pos];
        let file = expanded_refs.get(pos + 1)?;

        let file = if expanded_refs[pos] == ">>" {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(file)
                .ok()?
        } else {
            File::create(file).ok()?
        };

        cmd.args(real_args).stdout(Stdio::from(file));
    } else {
        cmd.args(&expanded_refs);
    }

    let mut child = cmd.spawn().ok()?;
    *current_pid.lock().unwrap() = Some(child.id());

    let status = child.wait().ok()?;
    *current_pid.lock().unwrap() = None;

    status.code().or(Some(130))
}
