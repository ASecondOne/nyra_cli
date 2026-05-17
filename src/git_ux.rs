use std::process::Command;

pub fn git_prompt() -> Option<String> {
    let branch = git_out(["branch", "--show-current"])?;
    if branch.is_empty() {
        return None;
    }

    let status = git_out(["status", "--porcelain"])?;
    let upstream =
        git_out(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]).unwrap_or_default();

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
    let out = Command::new("git").args(args).output().ok()?;

    if !out.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
