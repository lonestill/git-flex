use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use rayon::prelude::*;
use sha1::{Digest, Sha1};

pub struct VanityResult {
    pub original_hash: String,
    pub new_hash: String,
    pub attempts: u64,
    pub elapsed: f64,
    pub hashrate: f64,
    pub branch: Option<String>,
}

pub fn parse_hex_prefix(prefix: &str) -> Result<(Vec<u8>, bool), String> {
    let trimmed = prefix.trim();
    let cleaned = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed)
        .to_lowercase();

    if cleaned.is_empty() {
        return Err("Prefix cannot be empty".to_string());
    }
    if cleaned.len() > 40 {
        return Err("Prefix cannot be longer than 40 hex characters".to_string());
    }
    for c in cleaned.chars() {
        if !c.is_ascii_hexdigit() {
            return Err(format!("Invalid character '{}' in hex prefix. Must be [0-9a-fA-F]", c));
        }
    }

    let mut pattern_bytes = Vec::new();
    let is_odd = cleaned.len() % 2 != 0;
    let chars: Vec<char> = cleaned.chars().collect();
    
    for i in (0..chars.len() - if is_odd { 1 } else { 0 }).step_by(2) {
        let hi = chars[i].to_digit(16).unwrap() as u8;
        let lo = chars[i + 1].to_digit(16).unwrap() as u8;
        pattern_bytes.push((hi << 4) | lo);
    }
    if is_odd {
        let hi = chars.last().unwrap().to_digit(16).unwrap() as u8;
        pattern_bytes.push(hi << 4);
    }

    Ok((pattern_bytes, is_odd))
}

fn matches_prefix(digest: &[u8; 20], pattern: &[u8], is_odd: bool) -> bool {
    let full_bytes = if is_odd { pattern.len() - 1 } else { pattern.len() };
    if digest[..full_bytes] != pattern[..full_bytes] {
        return false;
    }
    if is_odd {
        let last_idx = pattern.len() - 1;
        if (digest[last_idx] & 0xF0) != pattern[last_idx] {
            return false;
        }
    }
    true
}

pub fn mine_vanity(
    prefix_str: &str,
    commit_ref: &str,
    use_trailer: bool,
    dry_run: bool,
) -> Result<VanityResult, String> {
    let (pattern_bytes, is_odd) = parse_hex_prefix(prefix_str)?;
    let clean_prefix = prefix_str
        .trim()
        .strip_prefix("0x")
        .or_else(|| prefix_str.trim().strip_prefix("0X"))
        .unwrap_or(prefix_str.trim())
        .to_lowercase();

    let git_rev = Command::new("git")
        .args(["rev-parse", "--verify", commit_ref])
        .output()
        .map_err(|e| format!("git error: {}", e))?;

    if !git_rev.status.success() {
        return Err(format!("Failed to resolve git commit '{}'", commit_ref));
    }
    let base_hash = String::from_utf8_lossy(&git_rev.stdout).trim().to_string();

    let branch_ref_out = Command::new("git")
        .args(["symbolic-ref", "-q", "--short", "HEAD"])
        .output();
    let current_branch = branch_ref_out
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

    if base_hash.starts_with(&clean_prefix) {
        return Ok(VanityResult {
            original_hash: base_hash.clone(),
            new_hash: base_hash,
            attempts: 0,
            elapsed: 0.0,
            hashrate: 0.0,
            branch: current_branch,
        });
    }

    let cat_output = Command::new("git")
        .args(["cat-file", "commit", commit_ref])
        .output()
        .map_err(|e| format!("git cat-file error: {}", e))?;

    if !cat_output.status.success() {
        return Err(format!("Failed to read git commit '{}'", commit_ref));
    }
    let mut raw_body = cat_output.stdout;

    if raw_body.last() != Some(&b'\n') {
        raw_body.push(b'\n');
    }

    let nonce_len: usize = if use_trailer { 32 } else { 64 };
    let prefix_trailer = if use_trailer {
        b"\nVanity-Nonce: ".to_vec()
    } else {
        b"\n\n".to_vec()
    };

    let mut base_content = raw_body;
    base_content.extend_from_slice(&prefix_trailer);

    let total_body_len = base_content.len() + nonce_len + 1;
    let git_header = format!("commit {}\0", total_body_len);

    let mut full_prefix = Vec::new();
    full_prefix.extend_from_slice(git_header.as_bytes());
    full_prefix.extend_from_slice(&base_content);

    let midstate_len = (full_prefix.len() / 64) * 64;
    let mut midstate = Sha1::new();
    midstate.update(&full_prefix[..midstate_len]);

    let tail_prefix = full_prefix[midstate_len..].to_vec();

    let found = Arc::new(AtomicBool::new(false));
    let total_attempts = Arc::new(AtomicU64::new(0));
    let winning_commit = Arc::new(Mutex::new(None));

    let start_time = Instant::now();
    let num_threads = rayon::current_num_threads();

    (0..num_threads).into_par_iter().for_each(|thread_id| {
        let thread_found = Arc::clone(&found);
        let thread_attempts = Arc::clone(&total_attempts);
        let thread_winning = Arc::clone(&winning_commit);
        let base_hasher = midstate.clone();
        
        let mut tail_buf = tail_prefix.clone();
        let tail_orig_len = tail_buf.len();
        tail_buf.resize(tail_orig_len + nonce_len + 1, b'\n');

        let mut local_counter: u64 = 0;
        let mut nonce: u64 = 0;

        if use_trailer {
            // Encode thread_id into the first 4 hex chars so each thread mines disjoint space
            let tid_hex = format!("{:04x}", (thread_id & 0xFFFF) as u16);
            tail_buf[tail_orig_len..tail_orig_len + 4].copy_from_slice(tid_hex.as_bytes());
            let hex_chars = b"0123456789abcdef";

            while !thread_found.load(Ordering::Relaxed) {
                let mut n = nonce;
                for i in 4..nonce_len {
                    tail_buf[tail_orig_len + i] = hex_chars[(n & 0xF) as usize];
                    n >>= 4;
                }

                let mut hasher = base_hasher.clone();
                hasher.update(&tail_buf);
                let digest: [u8; 20] = hasher.finalize().into();

                if matches_prefix(&digest, &pattern_bytes, is_odd) {
                    thread_found.store(true, Ordering::SeqCst);
                    
                    let mut full_body = base_content.clone();
                    for i in 0..nonce_len {
                        full_body.push(tail_buf[tail_orig_len + i]);
                    }
                    full_body.push(b'\n');

                    let mut lock = thread_winning.lock().unwrap();
                    *lock = Some(full_body);
                    break;
                }

                nonce = nonce.wrapping_add(1);
                local_counter += 1;

                if local_counter % 262_144 == 0 {
                    thread_attempts.fetch_add(262_144, Ordering::Relaxed);
                }
            }
        } else {
            // Invisible whitespace mode: b" \t"
            // Use 64 chars: first 16 chars uniquely encode thread_id
            let ws = [b' ', b'\t'];
            for bit in 0..16 {
                tail_buf[tail_orig_len + bit] = ws[((thread_id >> bit) & 1) as usize];
            }

            while !thread_found.load(Ordering::Relaxed) {
                let mut n = nonce;
                for i in 16..nonce_len {
                    tail_buf[tail_orig_len + i] = ws[(n & 1) as usize];
                    n >>= 1;
                }

                let mut hasher = base_hasher.clone();
                hasher.update(&tail_buf);
                let digest: [u8; 20] = hasher.finalize().into();

                if matches_prefix(&digest, &pattern_bytes, is_odd) {
                    thread_found.store(true, Ordering::SeqCst);
                    
                    let mut full_body = base_content.clone();
                    for i in 0..nonce_len {
                        full_body.push(tail_buf[tail_orig_len + i]);
                    }
                    full_body.push(b'\n');

                    let mut lock = thread_winning.lock().unwrap();
                    *lock = Some(full_body);
                    break;
                }

                nonce = nonce.wrapping_add(1);
                local_counter += 1;

                if local_counter % 262_144 == 0 {
                    thread_attempts.fetch_add(262_144, Ordering::Relaxed);
                }
            }
        }

        thread_attempts.fetch_add(local_counter % 262_144, Ordering::Relaxed);
    });

    let elapsed = start_time.elapsed().as_secs_f64();
    let attempts = total_attempts.load(Ordering::Relaxed);
    let hashrate = if elapsed > 0.0 { (attempts as f64) / elapsed / 1_000_000.0 } else { 0.0 };

    let winning_body = match winning_commit.lock().unwrap().take() {
        Some(body) => body,
        None => return Err("Mining interrupted or failed".to_string()),
    };

    let mut child = Command::new("git")
        .args(["hash-object", "-t", "commit", "-w", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git hash-object: {}", e))?;

    child.stdin.as_mut().unwrap().write_all(&winning_body).map_err(|e| format!("stdin error: {}", e))?;
    let output = child.wait_with_output().map_err(|e| format!("wait error: {}", e))?;
    let new_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !dry_run {
        if let Some(ref branch) = current_branch {
            let _ = Command::new("git").args(["update-ref", &format!("refs/heads/{}", branch), &new_hash]).status();
        } else {
            let _ = Command::new("git").args(["update-ref", "HEAD", &new_hash]).status();
        }
    }

    Ok(VanityResult {
        original_hash: base_hash,
        new_hash,
        attempts,
        elapsed,
        hashrate,
        branch: current_branch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_prefix_valid() {
        let (pattern, is_odd) = parse_hex_prefix("dead").unwrap();
        assert_eq!(pattern, vec![0xDE, 0xAD]);
        assert!(!is_odd);

        let (pattern, is_odd) = parse_hex_prefix("0x1337c").unwrap();
        assert_eq!(pattern, vec![0x13, 0x37, 0xC0]);
        assert!(is_odd);
    }

    #[test]
    fn test_parse_hex_prefix_invalid() {
        assert!(parse_hex_prefix("").is_err());
        assert!(parse_hex_prefix("xyz").is_err());
    }

    #[test]
    fn test_matches_prefix() {
        let mut digest = [0u8; 20];
        digest[0] = 0xDE;
        digest[1] = 0xAD;
        digest[2] = 0xBE;

        let (pat1, is_odd1) = parse_hex_prefix("dead").unwrap();
        assert!(matches_prefix(&digest, &pat1, is_odd1));

        let (pat2, is_odd2) = parse_hex_prefix("deadb").unwrap();
        assert!(matches_prefix(&digest, &pat2, is_odd2));

        let (pat3, is_odd3) = parse_hex_prefix("deadc").unwrap();
        assert!(!matches_prefix(&digest, &pat3, is_odd3));
    }
}
