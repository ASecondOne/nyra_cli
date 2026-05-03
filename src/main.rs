use std::{
    borrow::Cow, cell::{Cell, RefCell}, fs::{File, OpenOptions}, path::{Path, PathBuf}, process::{Command, Stdio}, sync::{Arc, Mutex}
};

use colored::Colorize;
use nix::{
    sys::signal::{kill, Signal as NixSignal},
    unistd::Pid,
};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch, Reedline, Signal};

#[derive(Clone)]
struct Cmd {
    name: String,
    path: PathBuf,
}

struct NyPrompt {
    last_code: Cell<Option<i32>>,
    git_dir:  RefCell<Option<String>>
}

impl Prompt for NyPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        match &*self.git_dir.borrow() {
            Some(branch) => Cow::Owned(format!(
                "{} {} ",
                "nyracli".purple(),
                format!("({branch})>").bright_black()
            )),
            None => Cow::Owned(format!("{} ", "nyracli>".purple())),
        }
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        match self.last_code.get() {
            Some(0) => Cow::Owned("[0]".green().to_string()),
            Some(code) => Cow::Owned(format!("[{code}]").red().to_string()),
            None => Cow::Borrowed(""),
        }
    }

    fn render_prompt_indicator(&self, _: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_history_search_indicator(&self, _: PromptHistorySearch) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
}

fn main() {
    let commands = load_commands();
    let mut line_editor = Reedline::create();

    let prompt = NyPrompt {
        last_code: Cell::new(None),
        git_dir: RefCell::new(None),
    };

    let current_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

    {
        let current_pid = current_pid.clone();

        ctrlc::set_handler(move || {
            if let Some(pid) = *current_pid.lock().unwrap() {
                let _ = kill(Pid::from_raw(pid as i32), NixSignal::SIGINT);
            }
        }).unwrap();
    }

    loop {
        *prompt.git_dir.borrow_mut() = git_prompt();

        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(input)) => {
                let input = input.trim();

                if input.is_empty() {
                    continue;
                }

                let parts = match shell_words::split(input) {
                    Ok(parts) => parts,
                    Err(e) => {
                        println!("parse error: {e}");
                        prompt.last_code.set(Some(2));
                        continue;
                    }
                };
                
                match parts[0].as_str() {
                    "exit" => break,

                    "print_commands" => {
                        for (i, command) in commands.iter().enumerate() {
                            println!("{}: {} {}", i, command.name, command.path.display());
                        }
                    }

                    "cd" => {
                        let raw = if parts.len() > 1 {
                            parts[1..].join(" ")
                        } else {
                            "~".to_string()
                        };

                        let path = if raw.starts_with('~') {
                            let home = std::env::var("HOME").unwrap();
                            raw.replacen('~', &home, 1)
                        } else {
                            raw
                        };

                        match std::env::set_current_dir(&path) {
                            Ok(_) => prompt.last_code.set(Some(0)),
                            Err(e) => {
                                println!("cd: {e}");
                                prompt.last_code.set(Some(1));
                            }
                        }
                    }

                    "openhere" => {
                        match Command::new("xdg-open").arg(".").spawn() {
                            Ok(_) => prompt.last_code.set(Some(0)),
                            Err(e) => {
                                println!("openhere: {e}");
                                prompt.last_code.set(Some(1));
                            }
                        }
                    }

                    _ => {
                        if let Some(cmd) = any_match_exists(&commands, |c| c == parts[0]) {
                            let args: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
                            let code = run_command(cmd, &args, current_pid.clone());
                            prompt.last_code.set(code);
                        } else {
                            println!("Nothing found");
                            prompt.last_code.set(Some(127));
                        }
                    }
                }
            }

            Ok(Signal::CtrlD) => {
                println!();
                break;
            }

            Ok(_) => {}

            Err(e) => {
                eprintln!("reedline error: {e}");
                break;
            }
        }
    }
}

fn any_match_exists<F>(cmds: &[Cmd], f: F) -> Option<Cmd>
where
    F: Fn(&str) -> bool,
{
    cmds.iter().find(|c| f(&c.name)).cloned()
}

fn load_commands() -> Vec<Cmd> {
    std::env::var("PATH")
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

fn run_command(
    command: Cmd,
    args: &[&str],
    current_pid: Arc<Mutex<Option<u32>>>,
) -> Option<i32> {
    let mut cmd = Command::new(command.path);

    if let Some(pos) = args.iter().position(|&r| r == ">" || r == ">>") {
        let real_args = &args[..pos];
        let file = args.get(pos + 1)?;

        let file = if args[pos] == ">>" {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(file)
                .ok()?
        } else {
            File::create(file).ok()?
        };

        cmd.args(real_args)
            .stdout(Stdio::from(file));
    } else {
        cmd.args(args);
    }

    let mut child = cmd.spawn().ok()?;

    *current_pid.lock().unwrap() = Some(child.id());

    let status = child.wait().ok()?;

    *current_pid.lock().unwrap() = None;

    status.code().or(Some(130))
}

fn git_prompt() -> Option<String> {
    let branch = git_out(["branch", "--show-current"])?;
    if branch.is_empty() {
        return None;
    }

    let status = git_out(["status", "--porcelain"])?;
    let upstream = git_out(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .unwrap_or_default();

    let mut marker = String::new();

    if status.lines().any(|l| l.starts_with("??")) {
        marker.push('+'); // untracked / unadded
    }

    if status.lines().any(|l| !l.starts_with("??")) {
        marker.push('*'); // changed / uncommitted
    }

    let nums: Vec<&str> = upstream.split_whitespace().collect();
    if nums.len() == 2 {
        if nums[0] != "0" {
            marker.push('↑'); // pushable commits
        }
        if nums[1] != "0" {
            marker.push('↓'); // pullable commits
        }
    }

    Some(format!("{branch}{marker}"))
}

fn git_out<const N: usize>(args: [&str; N]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}