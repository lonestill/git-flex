use chrono::{Datelike, Duration, Utc};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn get_glyph(c: char) -> Option<[u8; 5]> {
    let c = c.to_ascii_uppercase();
    match c {
        'A' => Some([0x7E, 0x11, 0x11, 0x11, 0x7E]),
        'B' => Some([0x7F, 0x49, 0x49, 0x49, 0x36]),
        'C' => Some([0x3E, 0x41, 0x41, 0x41, 0x22]),
        'D' => Some([0x7F, 0x41, 0x41, 0x22, 0x1C]),
        'E' => Some([0x7F, 0x49, 0x49, 0x49, 0x41]),
        'F' => Some([0x7F, 0x09, 0x09, 0x09, 0x01]),
        'G' => Some([0x3E, 0x41, 0x49, 0x49, 0x7A]),
        'H' => Some([0x7F, 0x08, 0x08, 0x08, 0x7F]),
        'I' => Some([0x00, 0x41, 0x7F, 0x41, 0x00]),
        'J' => Some([0x20, 0x40, 0x41, 0x3F, 0x01]),
        'K' => Some([0x7F, 0x08, 0x14, 0x22, 0x41]),
        'L' => Some([0x7F, 0x40, 0x40, 0x40, 0x40]),
        'M' => Some([0x7F, 0x02, 0x0C, 0x02, 0x7F]),
        'N' => Some([0x7F, 0x04, 0x08, 0x10, 0x7F]),
        'O' => Some([0x3E, 0x41, 0x41, 0x41, 0x3E]),
        'P' => Some([0x7F, 0x09, 0x09, 0x09, 0x06]),
        'Q' => Some([0x3E, 0x41, 0x51, 0x21, 0x5E]),
        'R' => Some([0x7F, 0x09, 0x19, 0x29, 0x46]),
        'S' => Some([0x46, 0x49, 0x49, 0x49, 0x31]),
        'T' => Some([0x01, 0x01, 0x7F, 0x01, 0x01]),
        'U' => Some([0x3F, 0x40, 0x40, 0x40, 0x3F]),
        'V' => Some([0x1F, 0x20, 0x40, 0x20, 0x1F]),
        'W' => Some([0x7F, 0x20, 0x18, 0x20, 0x7F]),
        'X' => Some([0x63, 0x14, 0x08, 0x14, 0x63]),
        'Y' => Some([0x07, 0x08, 0x70, 0x08, 0x07]),
        'Z' => Some([0x61, 0x51, 0x49, 0x45, 0x43]),
        '0' => Some([0x3E, 0x51, 0x49, 0x45, 0x3E]),
        '1' => Some([0x00, 0x42, 0x7F, 0x40, 0x00]),
        '2' => Some([0x42, 0x61, 0x51, 0x49, 0x46]),
        '3' => Some([0x21, 0x41, 0x45, 0x4B, 0x31]),
        '4' => Some([0x18, 0x14, 0x12, 0x7F, 0x10]),
        '5' => Some([0x27, 0x45, 0x45, 0x45, 0x39]),
        '6' => Some([0x3C, 0x4A, 0x49, 0x49, 0x30]),
        '7' => Some([0x01, 0x71, 0x09, 0x05, 0x03]),
        '8' => Some([0x36, 0x49, 0x49, 0x49, 0x36]),
        '9' => Some([0x06, 0x49, 0x49, 0x29, 0x1E]),
        ' ' => Some([0x00, 0x00, 0x00, 0x00, 0x00]),
        '!' => Some([0x00, 0x00, 0x5F, 0x00, 0x00]),
        '♥' | '<' => Some([0x0C, 0x1E, 0x3E, 0x1E, 0x0C]),
        _ => None,
    }
}

pub fn render_text_to_grid(text: &str) -> [[bool; 52]; 7] {
    let mut grid = [[false; 52]; 7];
    
    // Calculate total width of valid glyphs (5 cols per glyph + 1 col space between)
    let valid_count = text.chars().filter(|c| get_glyph(*c).is_some()).count();
    let total_width = if valid_count > 0 { valid_count * 6 - 1 } else { 0 };

    // Center text cleanly across all 52 columns of GitHub contribution calendar
    let mut col_offset = if total_width < 52 {
        (52usize.saturating_sub(total_width)) / 2
    } else {
        0
    };

    for ch in text.chars() {
        if let Some(glyph) = get_glyph(ch) {
            for col in 0..5 {
                let actual_col = col_offset + col;
                if actual_col >= 52 {
                    break;
                }
                let bits = glyph[col];
                for row in 0..7 {
                    if (bits & (1 << row)) != 0 {
                        grid[row][actual_col] = true;
                    }
                }
            }
            col_offset += 6;
            if col_offset >= 52 {
                break;
            }
        }
    }

    grid
}

pub fn clean_branch(branch: &str, author_name: &str, author_email: &str) -> Result<String, String> {
    let head_exists = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !head_exists {
        return Ok("EMPTY".to_string());
    }

    let tree_out = Command::new("git")
        .args(["rev-parse", "HEAD^{tree}"])
        .output()
        .map_err(|e| format!("rev-parse tree failed: {}", e))?;

    if !tree_out.status.success() {
        return Err("Failed to resolve HEAD tree".to_string());
    }
    let tree_sha = String::from_utf8_lossy(&tree_out.stdout).trim().to_string();

    let commit_out = Command::new("git")
        .args(["commit-tree", &tree_sha, "-m", "chore: clean repository state"])
        .env("GIT_AUTHOR_NAME", author_name)
        .env("GIT_AUTHOR_EMAIL", author_email)
        .env("GIT_COMMITTER_NAME", author_name)
        .env("GIT_COMMITTER_EMAIL", author_email)
        .output()
        .map_err(|e| format!("commit-tree failed: {}", e))?;

    if !commit_out.status.success() {
        return Err("Failed to create clean commit".to_string());
    }
    let new_commit = String::from_utf8_lossy(&commit_out.stdout).trim().to_string();

    let update = Command::new("git")
        .args(["update-ref", &format!("refs/heads/{}", branch), &new_commit])
        .status()
        .map_err(|e| format!("update-ref failed: {}", e))?;

    if !update.success() {
        return Err("Failed to update branch ref".to_string());
    }

    let _ = Command::new("git").args(["reset", "--hard", &new_commit]).status();

    Ok(new_commit)
}

pub fn paint_heatmap(
    branch: &str,
    text: &str,
    intensity: usize,
    year: Option<i32>,
    fill_background: bool,
    clean_first: bool,
    author_name: &str,
    author_email: &str,
) -> Result<usize, String> {
    if let Some(y) = year {
        if y < 1969 {
            return Err("Years before 1969 are not supported".to_string());
        }
    }

    if clean_first {
        clean_branch(branch, author_name, author_email)?;
    }

    let grid = render_text_to_grid(text);

    let now = Utc::now();
    let today = now.date_naive();

    let (start_date, max_cols, filter_year) = match year {
        Some(y) => {
            let jan_1 = chrono::NaiveDate::from_ymd_opt(y, 1, 1)
                .ok_or_else(|| format!("Invalid year: {}", y))?;
            let days_since_sun = jan_1.weekday().num_days_from_sunday() as i64;
            let start = jan_1 - Duration::days(days_since_sun);
            (start, 53, Some(y))
        }
        None => {
            let days_since_sunday = now.weekday().num_days_from_sunday() as i64;
            let most_recent_sunday = today - Duration::days(days_since_sunday);
            let start = most_recent_sunday - Duration::weeks(51);
            (start, 52, None)
        }
    };

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

    let mut total_commits = 0;
    let mut is_first = true;

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        let mut writer = std::io::BufWriter::new(stdin);

        for col in 0..max_cols {
            for row in 0..7 {
                let commit_day = start_date + Duration::weeks(col as i64) + Duration::days(row as i64);

                // If targeting a specific calendar year, stay strictly within that year
                if let Some(y) = filter_year {
                    if commit_day.year() != y {
                        continue;
                    }
                }

                // Never commit into future dates (GitHub ignores future commits until that day arrives)
                if commit_day > today {
                    continue;
                }

                let is_pixel = if col < 52 { grid[row][col] } else { false };
                let count = if is_pixel {
                    intensity.max(1)
                } else if fill_background {
                    1
                } else {
                    0
                };

                if count == 0 {
                    continue;
                }

                let base_dt = commit_day.and_hms_opt(12, 0, 0).unwrap().and_utc();
                let base_ts = base_dt.timestamp();

                for k in 0..count {
                    let ts = base_ts + (k as i64 * 30);
                    let msg = if is_pixel {
                        format!("art: pixel ({}, {}) #{}", col, row, k)
                    } else {
                        format!("chore: canvas base ({}, {})", col, row)
                    };

                    let from_clause = if is_first && has_parent {
                        format!("from refs/heads/{}^0\n", branch)
                    } else {
                        String::new()
                    };
                    is_first = false;

                    let cmd = format!(
                        "commit refs/heads/{}\nauthor {} <{}> {} +0000\ncommitter {} <{}> {} +0000\ndata {}\n{}\n{}",
                        branch, author_name, author_email, ts, author_name, author_email, ts, msg.len(), msg, from_clause
                    );
                    writer.write_all(cmd.as_bytes()).map_err(|e| format!("write error: {}", e))?;
                    total_commits += 1;
                }
            }
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

    Ok(total_commits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_glyph_letters_and_digits() {
        assert!(get_glyph('A').is_some());
        assert!(get_glyph('z').is_some());
        assert!(get_glyph('0').is_some());
        assert!(get_glyph('9').is_some());
        assert!(get_glyph('♥').is_some());
        assert!(get_glyph('@').is_none());
    }

    #[test]
    fn test_render_text_grid_bounds() {
        let grid = render_text_to_grid("LZT");
        let mut active_count = 0;
        for r in 0..7 {
            for c in 0..52 {
                if grid[r][c] {
                    active_count += 1;
                }
            }
        }
        assert!(active_count > 0);

        // Long word should fit within bounds without panic
        let grid_long = render_text_to_grid("ABCDEFG");
        assert_eq!(grid_long.len(), 7);
        assert_eq!(grid_long[0].len(), 52);
    }
}
