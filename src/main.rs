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
    parser::{ChainPart, RunMode, parse_line},
    pipe::NyPipe,
    vars::Vars,
};

struct NyPrompt {
    last_code: Cell<Option<i32>>,
    git_dir: RefCell<Option<String>>,
    config: PromptConfig,
}

struct PromptConfig {
    symbol: String,
    show_exit_code: bool,
}

enum LineResult {
    Code(i32),
    Exit,
}

impl Prompt for NyPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        let folder = current_folder();

        match &*self.git_dir.borrow() {
            Some(branch) => Cow::Owned(format!(
                "{} {} {} ",
                folder.purple(),
                format!("({branch})").bright_black(),
                self.config.symbol.purple()
            )),
            None => Cow::Owned(format!(
                "{} {} ",
                folder.purple(),
                self.config.symbol.purple()
            )),
        }
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        if !self.config.show_exit_code {
            return Cow::Borrowed("");
        }

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
            ReedlineEvent::Edit(vec![EditCommand::Complete]),
        ]),
    );

    let edit_mode = Box::new(Emacs::new(keybindings));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

    let history = Box::new(load_history());

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
        config: prompt_config(),
    };

    let current_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

    {
        let current_pid = current_pid.clone();

        if let Err(error) = ctrlc::set_handler(move || {
            if let Ok(pid) = current_pid.lock()
                && let Some(pid) = *pid {
                    let _ = kill(Pid::from_raw(pid as i32), NixSignal::SIGINT);
                }
        }) {
            eprintln!("ctrlc: {error}");
        }
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

                let chain = match parse_line(input) {
                    Ok(chain) => chain,
                    Err(error) => {
                        println!("parse error: {error}");
                        prompt.last_code.set(Some(2));
                        continue;
                    }
                };

                match run_chain(
                    &chain,
                    &nycommand,
                    &mut env_vars,
                    &mut nyalias,
                    current_pid.clone(),
                    &mut last_cd_dir,
                ) {
                    Ok(LineResult::Code(code)) => prompt.last_code.set(Some(code)),
                    Ok(LineResult::Exit) => break,
                    Err(error) => {
                        println!("{error}");
                        prompt.last_code.set(Some(1));
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

fn run_chain(
    chain: &[ChainPart],
    nycommand: &NyCommand,
    env_vars: &mut Vars,
    nyalias: &mut NyAlias,
    current_pid: Arc<Mutex<Option<u32>>>,
    last_cd_dir: &mut Option<String>,
) -> Result<LineResult, String> {
    let mut last_code = 0;

    for command in chain {
        if !should_run(command.mode, last_code) {
            continue;
        }

        let parts = match nyalias.resolve_alias(&command.parts)? {
            Some(expanded) => expanded,
            None => command.parts.clone(),
        };

        match run_parts(
            parts,
            nycommand,
            env_vars,
            nyalias,
            current_pid.clone(),
            last_cd_dir,
        )? {
            LineResult::Code(code) => last_code = code,
            LineResult::Exit => return Ok(LineResult::Exit),
        }
    }

    Ok(LineResult::Code(last_code))
}

fn should_run(mode: RunMode, last_code: i32) -> bool {
    match mode {
        RunMode::Always => true,
        RunMode::OnSuccess => last_code == 0,
        RunMode::OnFailure => last_code != 0,
    }
}

fn run_parts(
    parts: Vec<String>,
    nycommand: &NyCommand,
    env_vars: &mut Vars,
    nyalias: &mut NyAlias,
    current_pid: Arc<Mutex<Option<u32>>>,
    last_cd_dir: &mut Option<String>,
) -> Result<LineResult, String> {
    let Some(name) = parts.first() else {
        return Err("missing command".to_string());
    };

    if is_redirection(name) {
        return Err("missing command before redirection".to_string());
    }

    match name.as_str() {
        "exit" => Ok(LineResult::Exit),

        "print_commands" => {
            for (i, command) in nycommand.get_commands().iter().enumerate() {
                println!("{}: {} {}", i, command.name, command.path.display());
            }

            Ok(LineResult::Code(0))
        }

        "cd" => Ok(LineResult::Code(run_cd(&parts, last_cd_dir))),

        "openhere" => match Command::new("xdg-open").arg(".").spawn() {
            Ok(_) => Ok(LineResult::Code(0)),
            Err(error) => Err(format!("openhere: {error}")),
        },

        "clear" => {
            print!("\x1B[2J\x1B[1;1H");
            Ok(LineResult::Code(0))
        }

        "export" | "set" => {
            if parts.len() == 1 {
                env_vars.print_vars();
                return Ok(LineResult::Code(0));
            }

            match env_vars.insert(&parts[1]) {
                Ok(code) => Ok(LineResult::Code(code)),
                Err(error) => Err(error),
            }
        }

        "unset" => {
            let Some(var) = parts.get(1) else {
                return Ok(LineResult::Code(print_error("unset: missing argument")));
            };

            match env_vars.remove(var) {
                Ok(code) => Ok(LineResult::Code(code)),
                Err(error) => Ok(LineResult::Code(print_error(&error))),
            }
        }

        "which" => {
            let Some(name) = parts.get(1) else {
                return Ok(LineResult::Code(print_error("which: missing argument")));
            };

            if let Some(cmd) = find_command(nycommand.get_commands(), name) {
                println!("{}", cmd.path.to_string_lossy());
                Ok(LineResult::Code(0))
            } else {
                Ok(LineResult::Code(print_error(&format!("{name} not found"))))
            }
        }

        "alias" => match nyalias.parse_input(parts) {
            Ok(()) => Ok(LineResult::Code(0)),
            Err(error) => Ok(LineResult::Code(print_error(&error))),
        },

        _ => {
            if let Some(pipe) = NyPipe::new(&parts)? {
                return pipe.run().map(LineResult::Code);
            }

            let Some(cmd) = find_command(nycommand.get_commands(), name) else {
                return Ok(LineResult::Code(command_not_found(
                    nycommand.get_commands(),
                    name,
                )));
            };

            let args: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
            run_command(cmd, &args, env_vars, current_pid).map(LineResult::Code)
        }
    }
}

fn run_cd(parts: &[String], last_cd_dir: &mut Option<String>) -> i32 {
    let raw = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        "~".to_string()
    };

    let path = if raw.starts_with('~') {
        raw.replacen('~', &home_dir(), 1)
    } else if raw == "-" {
        match last_cd_dir.clone() {
            Some(dir) => dir,
            None => return print_error("cd: No previous dir yet"),
        }
    } else {
        raw
    };

    *last_cd_dir = std::env::current_dir()
        .ok()
        .map(|dir| dir.to_string_lossy().to_string());

    match std::env::set_current_dir(&path) {
        Ok(_) => 0,
        Err(error) => print_error(&format!("cd: {error}")),
    }
}

fn find_command(cmds: &[Cmd], name: &str) -> Option<Cmd> {
    cmds.iter()
        .find(|cmd| cmd.name == name)
        .cloned()
        .or_else(|| {
            if name.contains('/') {
                Some(Cmd {
                    name: name.to_string(),
                    path: PathBuf::from(name),
                })
            } else {
                None
            }
        })
}

fn command_not_found(commands: &[Cmd], input: &str) -> i32 {
    println!("Nothing found");

    let suggestions = fuzzy_commands(commands, input, |score| score > 0.80);
    if !suggestions.is_empty() {
        println!("Did you mean one of these?");
        for suggestion in suggestions.iter().take(5) {
            println!("{suggestion}");
        }
    }

    127
}

fn is_redirection(s: &str) -> bool {
    matches!(s, "<" | ">" | ">>")
}

fn print_error(msg: &str) -> i32 {
    println!("{msg}");
    1
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
    PathBuf::from(home_dir()).join(".nyracli_history")
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

fn load_history() -> FileBackedHistory {
    match FileBackedHistory::with_file(1000, history_path()) {
        Ok(history) => history,
        Err(error) => {
            eprintln!("history: {error}");
            FileBackedHistory::new(1000).unwrap_or_default()
        }
    }
}

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
}

fn prompt_config() -> PromptConfig {
    PromptConfig {
        symbol: std::env::var("NYRA_PROMPT_SYMBOL").unwrap_or_else(|_| ">".to_string()),
        show_exit_code: env_flag("NYRA_SHOW_EXIT_CODE", true),
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => !matches!(value.as_str(), "0" | "false" | "False" | "FALSE"),
        Err(_) => default,
    }
}