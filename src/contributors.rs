use std::io::Write;
use std::process::{Command, Stdio};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Contributor {
    pub name: String,
    pub email: String,
    pub login: String,
}

pub const LEGENDS: &[(&str, u64, &str)] = &[
    ("Linus Torvalds", 1024025, "torvalds"),
    ("Guido van Rossum", 1525988, "gvanrossum"),
    ("Fabrice Bellard", 3496924, "bellard"),
    ("Evan You", 499550, "yyx990803"),
    ("Dan Abramov", 810438, "gaearon"),
    ("TJ Holowaychuk", 25254, "tj"),
    ("ThePrimeagen", 378822, "ThePrimeagen"),
    ("Brendan Eich", 407513, "BrendanEich"),
    ("Mitchell Hashimoto", 1299, "mitchellh"),
    ("David Heinemeier Hansson", 2741, "dhh"),
    ("John Resig", 1615, "jeresig"),
    ("Chris Lattner", 15152540, "lattner"),
    ("Rasmus Lerdorf", 47250, "rlerdorf"),
    ("Salvatore Sanfilippo", 65952, "antirez"),
    ("Igor Sysoev", 81216, "isysoev"),
    ("Bjarne Stroustrup", 1152060, "BjarneStroustrup"),
    ("Anders Hejlsberg", 972671, "ahejlsberg"),
    ("Rich Harris", 1162160, "Rich-Harris"),
    ("Rob Pike", 4324516, "robpike"),
    ("Ken Thompson", 10920436, "ken"),
    ("Yukihiro Matsumoto", 30733, "matz"),
    ("Ryan Dahl", 80, "ry"),
    ("Solomon Hykes", 13735, "shykes"),
    ("Miguel de Icaza", 36863, "migueldeicaza"),
    ("Vitalik Buterin", 2230894, "vbuterin"),
    ("Hal Finney", 75235, "halfinney"),
    ("Satya Nadella", 24252328, "satyanadella"),
    ("Alex Russell", 16724, "slightlyoff"),
    ("Paul Irish", 39191, "paulirish"),
    ("Addy Osmani", 110953, "addyosmani"),
];

#[derive(Deserialize)]
struct GhUserItem {
    id: u64,
    login: String,
}

pub fn get_legend_contributors() -> Vec<Contributor> {
    LEGENDS
        .iter()
        .map(|(name, id, login)| Contributor {
            name: name.to_string(),
            email: format!("{}+{}@users.noreply.github.com", id, login),
            login: login.to_string(),
        })
        .collect()
}

pub fn fetch_real_github_contributors(count: usize) -> Vec<Contributor> {
    let mut results = Vec::new();
    let batch_size = 100.min(count);
    
    // Pick random starting offsets in GitHub's user ID space
    let mut rng_seed = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(12345678) as u64;
    
    while results.len() < count {
        rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let since_offset = 1_000_000 + (rng_seed % 40_000_000);
        let remaining = count - results.len();
        let limit = batch_size.min(remaining);

        let endpoint = format!("/users?since={}&per_page={}", since_offset, limit);
        let output = Command::new("gh")
            .args(["api", &endpoint])
            .output();

        let mut fetched_in_round = 0;
        if let Ok(out) = output {
            if out.status.success() {
                if let Ok(items) = serde_json::from_slice::<Vec<GhUserItem>>(&out.stdout) {
                    for item in items {
                        // Skip bots and orgs
                        if !item.login.ends_with("[bot]") && !item.login.is_empty() {
                            results.push(Contributor {
                                name: item.login.clone(),
                                email: format!("{}+{}@users.noreply.github.com", item.id, item.login),
                                login: item.login,
                            });
                            fetched_in_round += 1;
                            if results.len() >= count {
                                break;
                            }
                        }
                    }
                }
            }
        }

        // If gh API didn't return anything (e.g. rate limit or offline), break to fallback
        if fetched_in_round == 0 {
            break;
        }
    }

    // If we still need more contributors, pull from curated legends
    if results.len() < count {
        let legends = get_legend_contributors();
        for l in legends {
            if !results.iter().any(|r| r.login == l.login) {
                results.push(l);
                if results.len() >= count {
                    break;
                }
            }
        }
    }

    results
}

pub fn inject_contributors(
    branch: &str,
    contributors: &[Contributor],
) -> Result<usize, String> {
    if contributors.is_empty() {
        return Err("No contributors specified".to_string());
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
        .map_err(|e| format!("fast-import spawn error: {}", e))?;

    let now_ts = chrono::Utc::now().timestamp();
    let mut total = 0;

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        let mut writer = std::io::BufWriter::new(stdin);

        for (i, c) in contributors.iter().enumerate() {
            let ts = now_ts + (i as i64 * 15);
            let msg = format!("feat(core): contribution from @{}\n", c.login);
            let from_clause = if i == 0 && has_parent {
                format!("from refs/heads/{}^0\n", branch)
            } else {
                String::new()
            };

            let cmd = format!(
                "commit refs/heads/{}\nauthor {} <{}> {} +0000\ncommitter {} <{}> {} +0000\ndata {}\n{}\n{}",
                branch,
                c.name,
                c.email,
                ts,
                c.name,
                c.email,
                ts,
                msg.len(),
                msg,
                from_clause
            );
            writer.write_all(cmd.as_bytes()).map_err(|e| format!("write error: {}", e))?;
            total += 1;
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

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legends_not_empty() {
        let legends = get_legend_contributors();
        assert!(legends.len() >= 25);
        for l in legends {
            assert!(!l.name.is_empty());
            assert!(l.email.contains("@users.noreply.github.com"));
        }
    }
}
