use chrono::{NaiveDate, Utc};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

pub struct StreakResult {
    pub days: usize,
    pub commits: usize,
    pub elapsed: f64,
    pub commits_per_sec: f64,
}

pub fn generate_streak(
    branch: &str,
    start_year: i32,
    commits_per_day: usize,
    author_name: &str,
    author_email: &str,
) -> Result<StreakResult, String> {
    if commits_per_day == 0 {
        return Err("Commits per day must be at least 1".to_string());
    }

    let start_date = NaiveDate::from_ymd_opt(start_year, 1, 1)
        .ok_or_else(|| format!("Invalid start year: {}", start_year))?;
    let today = Utc::now().date_naive();
    // Also cover today + 1 day to guarantee active streak across UTC+1 to UTC+14 timezones
    let end_date = today.succ_opt().unwrap_or(today);

    if start_date > end_date {
        return Err("Start date cannot be in the future".to_string());
    }

    let mut child = Command::new("git")
        .args(["fast-import", "--force", "--quiet", "--date-format=raw"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git fast-import: {}", e))?;

    let start_time = Instant::now();
    let mut total_commits = 0usize;
    let mut total_days = 0usize;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or("Failed to open stdin for git fast-import")?;
        let mut writer = std::io::BufWriter::with_capacity(512 * 1024, stdin);

        let mut cur = start_date;

        // If start year is 1970, prepend the Unix epoch commit at timestamp 0 (1970-01-01 00:00:00 UTC)
        // In GitHub's California PST timezone (UTC-8), ts 0 resolves to 1969-12-31 16:00:00 PST.
        // This ensures the streak is anchored seamlessly from 1969 across all 56 years.
        if start_year == 1970 {
            let msg = "chore: initialize epoch streak (1969/1970)\n";
            let commit_cmd = format!(
                "commit refs/heads/{}\nauthor {} <{}> 0 +0000\ncommitter {} <{}> 0 +0000\ndata {}\n{}\nM 644 inline .streak\ndata 14\nstreak active\n",
                branch,
                author_name,
                author_email,
                author_name,
                author_email,
                msg.len(),
                msg
            );
            writer
                .write_all(commit_cmd.as_bytes())
                .map_err(|e| format!("Pipe error: {}", e))?;
            total_commits += 1;
        }

        while cur <= end_date {
            total_days += 1;
            let date_str = cur.to_string();

            for c_idx in 0..commits_per_day {
                // Noon UTC timestamp ensures identical calendar date across UTC-11 to UTC+11 timezones
                let base_ts = cur
                    .and_hms_opt(12, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp();
                let ts = base_ts + (c_idx as i64);

                let msg = format!("chore: daily sync {}\n", date_str);
                let content = format!("{}\n", date_str);

                let commit_cmd = format!(
                    "commit refs/heads/{}\nauthor {} <{}> {} +0000\ncommitter {} <{}> {} +0000\ndata {}\n{}\nM 644 inline .streak\ndata {}\n{}",
                    branch,
                    author_name,
                    author_email,
                    ts,
                    author_name,
                    author_email,
                    ts,
                    msg.len(),
                    msg,
                    content.len(),
                    content
                );

                writer
                    .write_all(commit_cmd.as_bytes())
                    .map_err(|e| format!("Pipe error: {}", e))?;
                total_commits += 1;
            }

            cur = match cur.succ_opt() {
                Some(next) => next,
                None => break,
            };
        }

        writer.flush().map_err(|e| format!("Flush error: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("fast-import failed: {}", e))?;
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
            let _ = Command::new("git")
                .args(["reset", "--hard", &format!("refs/heads/{}", branch)])
                .status();
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let commits_per_sec = if elapsed > 0.0 {
        (total_commits as f64) / elapsed
    } else {
        0.0
    };

    Ok(StreakResult {
        days: total_days,
        commits: total_commits,
        elapsed,
        commits_per_sec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streak_validation() {
        let res = generate_streak("dummy", 2099, 1, "test", "test@test.com");
        assert!(res.is_err());

        let res0 = generate_streak("dummy", 2026, 0, "test", "test@test.com");
        assert!(res0.is_err());
    }
}
