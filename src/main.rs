use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
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
            Some(branch) => {
                Cow::Owned(format!("nyracli ({})> ", branch))
            }
            None => {
                Cow::Borrowed("nyracli> ")
            }
        }
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        match self.last_code.get() {
            Some(code) => Cow::Owned(format!("[{code}]")),
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

    *prompt.git_dir.borrow_mut() = check_git_folder();

    loop {
        *prompt.git_dir.borrow_mut() = check_git_folder();

        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(input)) => {
                let input = input.trim();

                if input.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = input.split_ascii_whitespace().collect();
                
                match parts[0] {
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

                    _ => {
                        if let Some(cmd) = any_match_exists(&commands, |c| c == parts[0]) {
                            match run_command(cmd, &parts[1..]) {
                                Ok(status) => prompt.last_code.set(status.code()),
                                Err(e) => {
                                    println!("Error: {e}");
                                    prompt.last_code.set(Some(1));
                                }
                            }
                        } else {
                            println!("Nothing found");
                            prompt.last_code.set(Some(127));
                        }
                    }
                }
            }

            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => {
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

fn run_command(command: Cmd, args: &[&str]) -> Result<ExitStatus, io::Error> {
    Command::new(command.path).args(args).status()
}

fn check_git_folder() -> Option<String> {
    if !Path::new(".git").exists() {
        return None;
    }

    let out = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();

    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}