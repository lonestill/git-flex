use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

pub struct BlastResult {
    pub count: usize,
    pub elapsed: f64,
    pub commits_per_sec: f64,
}

pub fn blast_commits(
    branch: &str,
    count: usize,
    message_prefix: &str,
    author_name: &str,
    author_email: &str,
) -> Result<BlastResult, String> {
    if count == 0 {
        return Err("Commit count must be greater than 0".to_string());
    }

    let has_parent = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{}", branch)])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let mut child = Command::new("git")
        .args(["fast-import", "--force", "--quiet", "--date-format=raw"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git fast-import: {}", e))?;

    let start = Instant::now();
    let now_ts = chrono::Utc::now().timestamp();

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin for git fast-import")?;
        let mut writer = std::io::BufWriter::new(stdin);

        for i in 1..=count {
            let ts = now_ts + (i as i64);
            let msg = format!("{} #{}\n", message_prefix, i);

            let from_clause = if i == 1 && has_parent {
                format!("from refs/heads/{}^0\n", branch)
            } else {
                String::new()
            };

            let commit_cmd = format!(
                "commit refs/heads/{}\nauthor {} <{}> {} +0000\ncommitter {} <{}> {} +0000\ndata {}\n{}\n{}",
                branch,
                author_name,
                author_email,
                ts,
                author_name,
                author_email,
                ts,
                msg.len(),
                msg,
                from_clause
            );
            writer.write_all(commit_cmd.as_bytes()).map_err(|e| format!("Pipe error: {}", e))?;
        }
        writer.flush().map_err(|e| format!("Flush error: {}", e))?;
    }

    let output = child.wait_with_output().map_err(|e| format!("fast-import failed: {}", e))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        if !err.trim().is_empty() {
            return Err(format!("git fast-import failed: {}", err));
        }
    }

    let current_branch = Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if let Some(ref cur) = current_branch {
        if cur == branch {
            let _ = Command::new("git").args(["reset", &format!("refs/heads/{}", branch)]).status();
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let commits_per_sec = if elapsed > 0.0 { (count as f64) / elapsed } else { 0.0 };

    Ok(BlastResult {
        count,
        elapsed,
        commits_per_sec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blast_zero_count_err() {
        let res = blast_commits("main", 0, "test", "User", "user@test.com");
        assert!(res.is_err());
    }
}
