use std::{fmt::Error, io, path::PathBuf, process::{Command, ExitStatus}};
use rustyline::DefaultEditor;

#[derive(Clone)]
struct Cmd {
    name: String,
    path: PathBuf,
}

fn main() {
    let commands = load_commands();
    let mut rl = DefaultEditor::new().unwrap();

    loop {
        let line = rl.readline("nyracli> ");

        match line {
            Ok(input) => {
                let input = input.trim();

                if input.is_empty() {
                    continue;
                }

                rl.add_history_entry(input).unwrap();

                let parts: Vec<&str> = input.split_ascii_whitespace().collect();

                match parts[0] {
                    "print_commands" => {
                        for (i, command) in commands.iter().enumerate() {
                            println!("{}: {} {}", i, command.name, command.path.display());
                        }
                    }
                    _ => {
                        if let Some(cmd) = any_match_exists(&commands, |c| c == parts[0]) {
                            // println!("Found: {} {}", cmd.name, cmd.path.display());

                            match run_command(cmd, &parts[1..]) {
                                Ok(status) => println!("Exited with: {}", status),
                                Err(e) => println!("Error: {e}"),
                            }
                        } else {
                            println!("Nothing found");
                        }
                    }
                }
            }

            Err(_) => break,
        }
    }
}

fn any_match_exists<F>(cmds: &[Cmd], f: F) -> Option<Cmd>
where
    F: Fn(&str) -> bool,
{
    cmds.iter()
        .find(|c| f(&c.name))
        .cloned()
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