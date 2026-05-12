use std::{
    borrow::Cow, cell::{Cell, RefCell}, collections::HashMap, fs::{File, OpenOptions}, path::PathBuf, process::{Command, Stdio}, sync::{Arc, Mutex}
};

use colored::Colorize;
use nix::{
    sys::signal::{kill, Signal as NixSignal},
    unistd::Pid,
};
use reedline::{ColumnarMenu, Completer, EditCommand, Emacs, FileBackedHistory, KeyCode, KeyModifiers, Keybindings, MenuBuilder, Prompt, PromptEditMode, PromptHistorySearch, Reedline, ReedlineEvent, ReedlineMenu, Signal, Suggestion, default_emacs_keybindings};
use strsim::jaro_winkler;

#[derive(Clone)]
struct Cmd {
    name: String,
    path: PathBuf,
}

struct NyPrompt {
    last_code: Cell<Option<i32>>,
    git_dir:  RefCell<Option<String>>
}

struct PipePart {
    cmd: String,
    args: Vec<String>
}

struct NyCompleter {
    commands: Vec<Cmd>
}

impl Completer for NyCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let before = &line[..pos];

        if before.starts_with("cd ") {
            return complete_cd(line, pos)
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
            span: reedline::Span {
                start: 0,
                end: pos,
            },
            append_whitespace: true,
            style: None,
            display_override: None,
            match_indices: None,
        })
        .collect()
    }
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

    let commands = load_commands();
    
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(KeyModifiers::ALT, KeyCode::Enter, 
        ReedlineEvent::Edit(vec![
            EditCommand::InsertNewline
        ]
        ));

    add_completion_keybinds(&mut keybindings);

    let edit_mode = Box::new(Emacs::new(keybindings));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

    let history = Box::new(
        FileBackedHistory::with_file(
            1000,
            history_path(),
        ).unwrap()
    );


    let mut line_editor = Reedline::create()
        .with_history(history)
        .with_completer(Box::new(NyCompleter {
            commands: commands.clone()
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
        }).unwrap();
    }

    let mut env_vars: HashMap<String, String> = HashMap::new();

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

                        last_cd_dir = Some(std::env::current_dir().unwrap().to_string_lossy().to_string());

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

                    "clear" => {
                        print!("\x1B[2J\x1B[1;1H");
                    }

                    "export" | "set" => {
                        if parts.len() == 1 {
                            for (k, v) in &env_vars {
                                println!("${} = {}", k, v)
                            }
                        } else {
                            if let Some(var) = parts.get(1) {
                                if let Some((key, value)) = var.split_once("=") {

                                    let key = key.strip_prefix('$').unwrap_or(key);

                                    env_vars.insert(key.to_string(), value.to_string());
                                    prompt.last_code.set(Some(0));
                                }
                            }
                        }
                    }

                    "unset" => {
                        if let Some(v) = parts.get(1) {
                            let var = v.strip_prefix("$").unwrap_or(v);
                            env_vars.remove(var);

                            prompt.last_code.set(Some(0));
                        } else {
                            println!("unset: missing argument");
                            prompt.last_code.set(Some(1));
                        }
                    }

                    "which" => {
                        if let Some(name) = parts.get(1) {
                            if let Some(c) = commands.iter().find(|c| c.name == *name) {
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

                    _ => {
                        if let Some(pipe_parts) = parse_pipe(&parts) {
                            let code = run_pipe(pipe_parts);
                            prompt.last_code.set(code);
                            continue;
                        }

                        if let Some(cmd) = any_match_exists(&commands, |c| c == parts[0]) {
                            let args: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
                            let code = run_command(cmd, &args, &env_vars, current_pid.clone());
                            prompt.last_code.set(code);
                        } else {
                            println!("Nothing found");
                            
                            let suggestions = fuzzy_commands(&commands, input, |score| score > 0.80);
                            if suggestions.len() > 0 {
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
    env_vars: &HashMap<String, String>,
    current_pid: Arc<Mutex<Option<u32>>>,
) -> Option<i32> {
    let expanded_args: Vec<String> = args
        .iter()
        .map(|arg| expand_vars(arg, env_vars))
        .collect();

    let expanded_refs: Vec<&str> = expanded_args.iter().map(String::as_str).collect();

    let mut cmd = Command::new(command.path);

    for (k, v) in env_vars {
        cmd.env(k, v);
    }

    if let Some(pos) = expanded_refs.iter().position(|&r| r == ">" || r == ">>") {
        let real_args = &expanded_refs[..pos];
        let file = expanded_refs.get(pos + 1)?;

        let file = if expanded_refs[pos] == ">>" {
            OpenOptions::new().create(true).append(true).open(file).ok()?
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

fn add_completion_keybinds(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
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

fn expand_vars(s: &str, env_vars: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        // ${VAR}
        if i + 1 < chars.len() && chars[i + 1] == '{' {
            let mut j = i + 2;

            while j < chars.len() && chars[j] != '}' {
                j += 1;
            }

            if j < chars.len() {
                let name: String = chars[i + 2..j].iter().collect();
                out.push_str(&get_var(&name, env_vars));
                i = j + 1;
                continue;
            }
        }

        // $VAR
        let mut j = i + 1;

        while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
            j += 1;
        }

        if j == i + 1 {
            out.push('$');
            i += 1;
            continue;
        }

        let name: String = chars[i + 1..j].iter().collect();
        out.push_str(&get_var(&name, env_vars));
        i = j;
    }

    out
}

fn get_var(name: &str, env_vars: &HashMap<String, String>) -> String {
    env_vars
        .get(name)
        .cloned()
        .or_else(|| std::env::var(name).ok())
        .unwrap_or_default()
}

fn current_folder() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or("/".into())
}

fn startup_banner() {
    let _ = Command::new("nyaofetch").status();
}

fn parse_pipe(parts: &[String]) -> Option<Vec<PipePart>> {
    let mut out = Vec::new();

    for chunk in parts.split(|p| p == "|") {
        if chunk.is_empty() {
            return None;
        }

        out.push(PipePart {
            cmd: chunk[0].clone(),
            args: chunk[1..].to_vec(),
        });
    }

    if out.len() > 1 { Some(out) } else { None }
}

fn run_pipe(parts: Vec<PipePart>) -> Option<i32> {
    let mut children = Vec::new();
    let mut prev_stdout = None;

    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1;

        let mut cmd = Command::new(&part.cmd);
        cmd.args(&part.args);

        if let Some(stdout) = prev_stdout.take() {
            cmd.stdin(Stdio::from(stdout));
        }

        if !is_last {
            cmd.stdout(Stdio::piped());
        }

        let mut child = cmd.spawn().ok()?;
        prev_stdout = child.stdout.take();

        children.push(child);
    }

    let mut last_code = Some(0);

    for mut child in children {
        let status = child.wait().ok()?;
        last_code = status.code().or(Some(130));
    }

    last_code
}

fn history_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or(".".into());
    PathBuf::from(home).join(".nyracli_history")
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
                span: reedline::Span {
                    start: 3,
                    end: pos,
                },
                append_whitespace: false,
                style: None,
                display_override: Some(name.clone()),
                match_indices: None,
            })
        })
        .collect()
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