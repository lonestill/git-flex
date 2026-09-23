use std::io::Write;
use std::process::{Command, Stdio};

pub const AI_COAUTHORS: &[(&str, &str)] = &[
    ("ChatGPT", "chatgpt@openai.com"),
    ("Claude", "claude@anthropic.com"),
    ("Gemini", "gemini@google.com"),
    ("DeepSeek", "deepseek@deepseek.com"),
    ("GitHub Copilot", "copilot@github.com"),
    ("Grok", "grok@x.ai"),
    ("Cursor", "cursor@cursor.com"),
    ("Meta Llama", "llama@meta.com"),
    ("Google Antigravity", "antigravity@google.com"),
    ("Mistral AI", "mistral@mistral.ai"),
    ("Qwen", "qwen@alibaba.com"),
];

pub fn stamp_ai_squad(commit_ref: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["cat-file", "commit", commit_ref])
        .output()
        .map_err(|e| format!("Failed to read commit: {}", e))?;

    if !output.status.success() {
        return Err("git cat-file failed to read commit".to_string());
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let parts: Vec<&str> = raw.splitn(2, "\n\n").collect();
    if parts.len() < 2 {
        return Err("Malformed git commit object (no header/body separator)".to_string());
    }

    let header = parts[0];
    let body = parts[1].trim_end();

    let mut trailers = String::new();
    for (name, email) in AI_COAUTHORS {
        if !body.contains(email) {
            trailers.push_str(&format!("Co-authored-by: {} <{}>\n", name, email));
        }
    }

    let new_body = if trailers.is_empty() {
        body.to_string() + "\n"
    } else {
        format!("{}\n\n{}", body, trailers)
    };

    let new_commit_content = format!("{}\n\n{}", header, new_body);

    let mut child = Command::new("git")
        .args(["hash-object", "-t", "commit", "-w", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git hash-object: {}", e))?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(new_commit_content.as_bytes())
        .map_err(|e| format!("Failed to write commit to stdin: {}", e))?;

    let res = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for git hash-object: {}", e))?;

    let new_hash = String::from_utf8_lossy(&res.stdout).trim().to_string();

    let is_head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None })
        .map(|h| commit_ref == "HEAD" || h == commit_ref || h.starts_with(commit_ref))
        .unwrap_or(false);

    if is_head {
        let branch_out = Command::new("git").args(["symbolic-ref", "-q", "HEAD"]).output().ok();
        if let Some(b) = branch_out {
            if b.status.success() {
                let branch = String::from_utf8_lossy(&b.stdout).trim().to_string();
                let _ = Command::new("git").args(["update-ref", &branch, &new_hash]).status();
            } else {
                let _ = Command::new("git").args(["update-ref", "HEAD", &new_hash]).status();
            }
        }
    }

    Ok(new_hash)
}

pub fn inject_contributor_commits(branch: &str) -> Result<usize, String> {
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
        .map_err(|e| format!("fast-import error: {}", e))?;

    let now_ts = chrono::Utc::now().timestamp();
    let mut count = 0;

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        let mut writer = std::io::BufWriter::new(stdin);

        for (i, (name, email)) in AI_COAUTHORS.iter().enumerate() {
            let ts = now_ts + (i as i64 * 10);
            let msg = format!("chore(ai): verified contributor profile for {}", name);
            let from_clause = if i == 0 && has_parent {
                format!("from refs/heads/{}^0\n", branch)
            } else {
                String::new()
            };

            let cmd = format!(
                "commit refs/heads/{}\nauthor {} <{}> {} +0000\ncommitter {} <{}> {} +0000\ndata {}\n{}\n{}",
                branch, name, email, ts, name, email, ts, msg.len(), msg, from_clause
            );
            writer.write_all(cmd.as_bytes()).map_err(|e| format!("write error: {}", e))?;
            count += 1;
        }
        writer.flush().map_err(|e| format!("flush error: {}", e))?;
    }

    let output = child.wait_with_output().map_err(|e| format!("fast-import wait error: {}", e))?;
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

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_coauthors_list_not_empty() {
        assert!(AI_COAUTHORS.len() >= 10);
        for (name, email) in AI_COAUTHORS {
            assert!(!name.is_empty());
            assert!(email.contains('@'));
        }
    }
}
