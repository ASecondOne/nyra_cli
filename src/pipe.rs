use std::process::{Command, Stdio};

struct PipePart {
    cmd: String,
    args: Vec<String>,
}

pub struct NyPipe {
    pipeparts: Vec<PipePart>
}

impl NyPipe {
    pub fn new(parts: &[String]) -> Option<Self> {
        if let Some(pipeparts) = Self::parse_pipe(parts) {
            return Some(Self { pipeparts });
        }
        
        None
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

    pub fn run(&self) -> Option<i32> {
        let mut children = Vec::new();
        let mut prev_stdout = None;

        for (i, part) in self.pipeparts.iter().enumerate() {
            let is_last = i == self.pipeparts.len() - 1;

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
}