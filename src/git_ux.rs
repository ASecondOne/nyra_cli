use std::process::Command;

pub fn git_prompt() -> Option<String> {
    let status = git_out(["status", "--porcelain=2", "--branch"])?;
    parse_git_prompt(&status)
}

fn parse_git_prompt(status: &str) -> Option<String> {
    let mut branch = None;
    let mut oid = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut has_untracked = false;
    let mut has_changes = false;

    for line in status.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            branch = match head {
                "(detached)" => None,
                _ => Some(head.to_string()),
            };
            continue;
        }

        if let Some(branch_oid) = line.strip_prefix("# branch.oid ") {
            if branch_oid != "(initial)" {
                oid = Some(branch_oid.to_string());
            }
            continue;
        }

        if let Some(ab) = line.strip_prefix("# branch.ab +") {
            if let Some((left, right)) = ab.split_once(" -") {
                ahead = left.parse().unwrap_or(0);
                behind = right.parse().unwrap_or(0);
            }
            continue;
        }

        if line.starts_with("? ") {
            has_untracked = true;
            continue;
        }

        if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
            has_changes = true;
        }
    }

    let branch = branch.or_else(|| oid.map(|id| id.chars().take(7).collect()))?;
    let mut marker = String::new();

    if has_untracked {
        marker.push('+');
    }

    if has_changes {
        marker.push('*');
    }

    if ahead != 0 {
        marker.push('↑');
    }

    if behind != 0 {
        marker.push('↓');
    }

    Some(format!("{branch}{marker}"))
}

fn git_out<const N: usize>(args: [&str; N]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;

    if !out.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_git_prompt;

    #[test]
    fn parses_branch_and_markers() {
        let status = "\
# branch.oid abcdef1234567890
# branch.head main
# branch.upstream origin/main
# branch.ab +2 -1
1 .M N... 100644 100644 100644 1234567 1234567 file.txt
? new.txt
";

        assert_eq!(parse_git_prompt(status), Some("main+*↑↓".to_string()));
    }

    #[test]
    fn parses_detached_head() {
        let status = "\
# branch.oid abcdef1234567890
# branch.head (detached)
";

        assert_eq!(parse_git_prompt(status), Some("abcdef1".to_string()));
    }
}
