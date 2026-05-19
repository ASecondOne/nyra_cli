use std::process::{Command, Stdio};

use crate::commands::apply_redirects;

struct PipePart {
    cmd: String,
    args: Vec<String>,
}

pub struct NyPipe {
    pipeparts: Vec<PipePart>,
}

impl NyPipe {
    pub fn new(parts: &[String]) -> Result<Option<Self>, String> {
        if !parts.iter().any(|part| part == "|") {
            return Ok(None);
        }

        let pipeparts = Self::parse_pipe(parts)?;
        Ok(Some(Self { pipeparts }))
    }

    fn parse_pipe(parts: &[String]) -> Result<Vec<PipePart>, String> {
        let mut out = Vec::new();

        for chunk in parts.split(|p| p == "|") {
            if chunk.is_empty() {
                return Err("pipe: missing command around '|'".to_string());
            }

            out.push(PipePart {
                cmd: chunk[0].clone(),
                args: chunk[1..].to_vec(),
            });
        }

        if out.len() > 1 {
            Ok(out)
        } else {
            Err("pipe: missing command around '|'".to_string())
        }
    }

    pub fn run(&self) -> Result<i32, String> {
        let mut children = Vec::new();
        let mut prev_stdout = None;

        for (i, part) in self.pipeparts.iter().enumerate() {
            let is_last = i == self.pipeparts.len() - 1;

            let mut cmd = Command::new(&part.cmd);

            if let Some(stdout) = prev_stdout.take() {
                cmd.stdin(Stdio::from(stdout));
            }

            let redirected = apply_redirects(&mut cmd, &part.args)?;
            cmd.args(&redirected.args);

            if redirected.stdout_redirected && !is_last {
                return Err("pipe: only the last command can redirect output".to_string());
            }

            if !is_last {
                cmd.stdout(Stdio::piped());
            }

            let mut child = cmd.spawn().map_err(|err| format!("{}: {err}", part.cmd))?;
            prev_stdout = child.stdout.take();

            children.push(child);
        }

        let mut last_code = 0;

        for mut child in children {
            let status = child.wait().map_err(|err| err.to_string())?;
            last_code = status.code().unwrap_or(130);
        }

        Ok(last_code)
    }
}
