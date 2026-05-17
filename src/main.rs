use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};

use colored::Colorize;
use nix::{
    sys::signal::{Signal as NixSignal, kill},
    unistd::Pid,
};
use reedline::{
    ColumnarMenu, EditCommand, Emacs, FileBackedHistory, KeyCode, KeyModifiers, MenuBuilder,
    Prompt, PromptEditMode, PromptHistorySearch, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    default_emacs_keybindings,
};
use strsim::jaro_winkler;

use nyra_cli::{
    alias::NyAlias,
    commands::{Cmd, NyCommand, run_command},
    completer::NyCompleter,
    git_ux::git_prompt,
    pipe::NyPipe,
    vars::Vars,
};

struct NyPrompt {
    last_code: Cell<Option<i32>>,
    git_dir: RefCell<Option<String>>,
}

impl Prompt for NyPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        match &*self.git_dir.borrow() {
            Some(branch) => Cow::Owned(format!(
                "{} {}{} ",
                current_folder().purple(),
                format!("({branch})").bright_black(),
                ">".purple()
            )),
            None => Cow::Owned(format!("{}{} ", current_folder().purple(), ">".purple())),
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
    // startup_banner();

    let mut nycommand = NyCommand::new();
    nycommand.load_commands();

    let mut env_vars = Vars::new();

    let mut nyalias = NyAlias::new();

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Enter,
        ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let edit_mode = Box::new(Emacs::new(keybindings));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

    let history = Box::new(FileBackedHistory::with_file(1000, history_path()).unwrap());

    let mut line_editor = Reedline::create()
        .with_history(history)
        .with_completer(Box::new(NyCompleter {
            commands: nycommand.get_commands().to_vec(),
        }))
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode);

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
        })
        .unwrap();
    }

    let mut last_cd_dir: Option<String> = None;

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

                let parts = match nyalias.resolve_alias(&parts) {
                    Ok(Some(expanded)) => expanded,
                    Ok(None) => parts,
                    Err(msg) => {
                        println!("{msg}");
                        prompt.last_code.set(Some(1));
                        continue;
                    }
                };

                match parts[0].as_str() {
                    "exit" => break,

                    "print_commands" => {
                        for (i, command) in nycommand.get_commands().iter().enumerate() {
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
                        } else if raw.starts_with('-') {
                            if last_cd_dir.is_some() {
                                last_cd_dir.clone().unwrap()
                            } else {
                                println!("cd: No previous dir yet");
                                prompt.last_code.set(Some(1));
                                continue;
                            }
                        } else {
                            raw
                        };

                        last_cd_dir = Some(
                            std::env::current_dir()
                                .unwrap()
                                .to_string_lossy()
                                .to_string(),
                        );

                        match std::env::set_current_dir(&path) {
                            Ok(_) => prompt.last_code.set(Some(0)),
                            Err(e) => {
                                println!("cd: {e}");
                                prompt.last_code.set(Some(1));
                            }
                        }
                    }

                    "openhere" => match Command::new("xdg-open").arg(".").spawn() {
                        Ok(_) => prompt.last_code.set(Some(0)),
                        Err(e) => {
                            println!("openhere: {e}");
                            prompt.last_code.set(Some(1));
                        }
                    },

                    "clear" => {
                        print!("\x1B[2J\x1B[1;1H");
                    }

                    "export" | "set" => {
                        if parts.len() == 1 {
                            env_vars.print_vars();
                        } else {
                            if let Some(var) = parts.get(1) {
                                match env_vars.insert(var) {
                                    Ok(code) => prompt.last_code.set(Some(code)),
                                    Err(error) => println!("{error}"),
                                }
                            }
                        }
                    }

                    "unset" => {
                        if let Some(v) = parts.get(1) {
                            match env_vars.remove(v) {
                                Ok(code) => prompt.last_code.set(Some(code)),
                                Err(error) => println!("{error}"),
                            }
                        } else {
                            println!("unset: missing argument");
                            prompt.last_code.set(Some(1));
                        }
                    }

                    "which" => {
                        if let Some(name) = parts.get(1) {
                            if let Some(c) =
                                nycommand.get_commands().iter().find(|c| c.name == *name)
                            {
                                println!("{}", c.path.to_string_lossy());
                                prompt.last_code.set(Some(0));
                            } else {
                                println!("{name} not found");
                                prompt.last_code.set(Some(1));
                            }
                        } else {
                            println!("which: missing argument");
                            prompt.last_code.set(Some(1));
                        }
                    }

                    "alias" => match nyalias.parse_input(parts) {
                        Ok(()) => prompt.last_code.set(Some(0)),
                        Err(msg) => {
                            println!("{msg}");
                            prompt.last_code.set(Some(1))
                        }
                    },

                    _ => {
                        if let Some(pipe) = NyPipe::new(&parts) {
                            let code = pipe.run();
                            prompt.last_code.set(code);
                            continue;
                        }

                        if let Some(cmd) =
                            any_match_exists(nycommand.get_commands(), |c| c == parts[0])
                        {
                            let args: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
                            let code = run_command(cmd, &args, &env_vars, current_pid.clone());
                            prompt.last_code.set(code);
                        } else {
                            println!("Nothing found");

                            let suggestions =
                                fuzzy_commands(nycommand.get_commands(), input, |score| {
                                    score > 0.80
                                });
                            if !suggestions.is_empty() {
                                println!("Did you mean one of these?");
                                for s in suggestions.iter().take(5) {
                                    print!("{s} ")
                                }
                            }

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

fn current_folder() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or("/".into())
}

fn _startup_banner() {
    let _ = Command::new("nyaofetch").status();
}

fn history_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or(".".into());
    PathBuf::from(home).join(".nyracli_history")
}

fn fuzzy_commands<F>(commands: &[Cmd], input: &str, keep: F) -> Vec<String>
where
    F: Fn(f64) -> bool,
{
    let mut scores: Vec<(String, f64)> = commands
        .iter()
        .map(|cmd| {
            let score = jaro_winkler(input, &cmd.name);
            (cmd.name.clone(), score)
        })
        .filter(|(_, score)| keep(*score))
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    scores.into_iter().map(|(name, _)| name).collect()
}
