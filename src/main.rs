mod ai_squad;
mod blaster;
pub mod contributors;
mod painter;
pub mod streak;
mod tui;
mod vanity;

use clap::{Parser, Subcommand};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(
    name = "git-flex",
    author = "lonestill",
    version = "0.1.0",
    about = "High-performance Git utility suite (TUI + CLI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Launch interactive terminal dashboard (TUI)
    Tui,

    /// Mine custom SHA-1 vanity commit hash using multi-threaded midstate precomputation
    Vanity {
        /// Target hex prefix (e.g. 0000000, deadbeef, 1337c0de)
        prefix: String,

        /// Target commit (default: HEAD)
        #[arg(short, long, default_value = "HEAD")]
        commit: String,

        /// Use visible git trailer (Vanity-Nonce: <hex>) instead of invisible whitespace
        #[arg(long)]
        trailer: bool,

        /// Dry run (do not update branch ref)
        #[arg(short, long)]
        dry_run: bool,

        /// Push to remote origin after mining
        #[arg(short = 'p', long)]
        push: bool,
    },

    /// Batch commit generation via streaming git fast-import pipeline
    Blast {
        /// Number of commits to generate
        #[arg(short = 'n', long, default_value_t = 1000)]
        count: usize,

        /// Target branch (default: current branch)
        #[arg(short, long)]
        branch: Option<String>,

        /// Commit message prefix
        #[arg(short, long, default_value = "chore: flex commit")]
        message: String,

        /// Push to remote origin after completion
        #[arg(short = 'p', long)]
        push: bool,
    },

    /// Rasterize text onto GitHub 7x52 contribution heatmap
    Draw {
        /// Text string to render (e.g. "LZT", "PRO", "AI")
        text: String,

        /// Target calendar year (e.g. 2025, 2024, 1970; default: rolling 52 weeks)
        #[arg(short = 'y', long)]
        year: Option<i32>,

        /// Target branch (default: current branch)
        #[arg(short, long)]
        branch: Option<String>,

        /// Commits per active cell (higher = darker intensity, default: 50 for max neon green)
        #[arg(short, long, default_value_t = 50)]
        intensity: usize,

        /// Fill unpainted days with 1 commit to create a Level-1 uniform green canvas
        #[arg(short = 'f', long, default_value_t = false)]
        fill_bg: bool,

        /// Clean previous commits on branch before painting (resets heatmap canvas)
        #[arg(short = 'c', long, default_value_t = false)]
        clean: bool,

        /// Push to remote origin after completion
        #[arg(short = 'p', long)]
        push: bool,
    },

    /// Wipe commit history and reset branch to single clean commit (clears GitHub heatmap)
    Clean {
        /// Target branch (default: current branch)
        #[arg(short, long)]
        branch: Option<String>,

        /// Push to remote origin after cleaning
        #[arg(short = 'p', long)]
        push: bool,
    },

    /// Append standard Co-authored-by metadata trailers to commit
    Ai {
        /// Target commit (default: HEAD)
        #[arg(short, long, default_value = "HEAD")]
        commit: String,

        /// Push to remote origin after completion
        #[arg(short = 'p', long)]
        push: bool,
    },

    /// Inflate repository contributors with arbitrary real GitHub accounts
    Contributors {
        /// Number of real contributors to inject (default: 25)
        #[arg(short = 'n', long, default_value_t = 25)]
        count: usize,

        /// Target branch (default: current branch)
        #[arg(short, long)]
        branch: Option<String>,

        /// Push to remote origin after completion
        #[arg(short = 'p', long)]
        push: bool,
    },

    /// Generate an unbroken daily commit streak from 1970/1969 to present (56 years)
    Streak {
        /// Starting year for streak (default: 1970)
        #[arg(short = 's', long, default_value_t = 1970)]
        start_year: i32,

        /// Number of commits per day (default: 1)
        #[arg(short = 'c', long, default_value_t = 1)]
        commits_per_day: usize,

        /// Target branch (default: current branch)
        #[arg(short, long)]
        branch: Option<String>,

        /// Push to remote origin after completion
        #[arg(short = 'p', long)]
        push: bool,
    },
}

pub fn get_git_config_user() -> (String, String) {
    let name = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "lonestill".to_string());

    let email = Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "lonestill@git-flex.dev".to_string());

    (name, email)
}

fn get_current_branch() -> String {
    Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "main".to_string())
}

fn get_git_remote() -> Option<String> {
    let remotes_out = Command::new("git").arg("remote").output().ok()?;
    if !remotes_out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&remotes_out.stdout);
    let all: Vec<String> = s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    if all.contains(&"origin".to_string()) {
        Some("origin".to_string())
    } else {
        all.into_iter().next()
    }
}

pub fn push_to_origin(branch: &str) -> Result<String, String> {
    let remote = match get_git_remote() {
        Some(r) => r,
        None => {
            let msg = "No remote configured. Run 'git remote add origin <url>' or 'gh repo create' first.";
            eprintln!("  remote: {}", msg);
            return Err(msg.to_string());
        }
    };

    // GitHub's push webhook and contribution worker truncate single push events to 1,000 commits.
    // When pushing large histories (>500 commits), we automatically chunk pushes into forward
    // batches of 500 commits so that GitHub receives separate PushEvents and ingests 100%
    // of all contributions across all calendar dates without truncation.
    let rev_list = Command::new("git")
        .args(["rev-list", "--reverse", &format!("refs/heads/{}", branch)])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).to_string())
            } else {
                None
            }
        });

    let commits: Vec<&str> = match rev_list {
        Some(ref s) => s.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect(),
        None => Vec::new(),
    };

    let chunk_size = 500usize;
    if commits.len() > chunk_size {
        println!(
            "  remote: large history detected ({} commits). Chunking push into {}-commit batches for 100% GitHub ingestion...",
            commits.len(),
            chunk_size
        );

        // Step 0: Anchor base commit
        let start_h = commits[0];
        let _ = Command::new("git")
            .args(["push", "-f", &remote, &format!("{}:refs/heads/{}", start_h, branch), "--quiet"])
            .status();

        // Steps 1..N: Fast-forward in chunks of 500
        for i in (chunk_size..commits.len()).step_by(chunk_size) {
            let h = commits[i];
            let _ = Command::new("git")
                .args(["push", &remote, &format!("{}:refs/heads/{}", h, branch), "--quiet"])
                .status();
        }

        // Final step: push HEAD of branch
        let res = Command::new("git")
            .args(["push", "-f", &remote, &format!("{}:refs/heads/{}", branch, branch)])
            .output();

        match res {
            Ok(o) if o.status.success() => {
                let msg = format!("successfully batch-pushed {} commits to {}/{}", commits.len(), remote, branch);
                println!("  remote: {}", msg);
                Ok(msg)
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                eprintln!("  remote: push failed: {}", err);
                Err(err)
            }
            Err(e) => {
                let err = format!("git push error: {}", e);
                eprintln!("  remote: {}", err);
                Err(err)
            }
        }
    } else {
        println!("  remote: pushing to {}/{}...", remote, branch);
        let res = Command::new("git")
            .args(["push", "-f", &remote, branch])
            .output();

        match res {
            Ok(o) if o.status.success() => {
                let msg = format!("successfully pushed to {}/{}", remote, branch);
                println!("  remote: {}", msg);
                Ok(msg)
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                eprintln!("  remote: push failed: {}", err);
                Err(err)
            }
            Err(e) => {
                let err = format!("git push error: {}", e);
                eprintln!("  remote: {}", err);
                Err(err)
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => {
            if let Err(e) = tui::run_tui() {
                eprintln!("error: failed to launch TUI: {}", e);
            }
        }
        Some(Commands::Vanity {
            prefix,
            commit,
            trailer,
            dry_run,
            push,
        }) => {
            println!("git-flex: mining vanity commit with prefix '{}'...", prefix);

            match vanity::mine_vanity(&prefix, &commit, trailer, dry_run) {
                Ok(res) => {
                    println!("  target prefix : {}", prefix);
                    println!("  mined hash    : {}", res.new_hash);
                    println!("  base hash     : {}", res.original_hash);
                    println!("  hashrate      : {:.2} MH/s ({} attempts in {:.3}s)", res.hashrate, res.attempts, res.elapsed);
                    if let Some(ref b) = res.branch {
                        if !dry_run {
                            println!("  ref updated   : refs/heads/{} -> {}", b, res.new_hash);
                        } else {
                            println!("  dry run       : refs/heads/{} not modified", b);
                        }
                        if push && !dry_run {
                            let _ = push_to_origin(b);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: vanity mining failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Blast {
            count,
            branch,
            message,
            push,
        }) => {
            let target_branch = branch.unwrap_or_else(get_current_branch);
            let (name, email) = get_git_config_user();

            println!("git-flex: streaming {} commits into refs/heads/{}...", count, target_branch);

            match blaster::blast_commits(&target_branch, count, &message, &name, &email) {
                Ok(res) => {
                    println!("  commits generated : {}", res.count);
                    println!("  throughput        : {:.0} commits/sec (in {:.3}s)", res.commits_per_sec, res.elapsed);
                    println!("  target ref        : refs/heads/{}", target_branch);
                    if push {
                        let _ = push_to_origin(&target_branch);
                    }
                }
                Err(e) => {
                    eprintln!("error: batch generation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Draw {
            text,
            year,
            branch,
            intensity,
            fill_bg,
            clean,
            push,
        }) => {
            let target_branch = branch.unwrap_or_else(get_current_branch);
            let (name, email) = get_git_config_user();

            match year {
                Some(y) => println!("git-flex: rasterizing '{}' onto {} contribution calendar...", text, y),
                None => println!("git-flex: rasterizing '{}' onto 52-week contribution matrix...", text),
            }
            if clean {
                println!("  clean  : resetting refs/heads/{} before painting...", target_branch);
            }
            if fill_bg {
                println!("  canvas : filling unpainted days with 1 commit/day (Level-1 uniform base)...");
            }

            match painter::paint_heatmap(&target_branch, &text, intensity, year, fill_bg, clean, &name, &email) {
                Ok(total) => {
                    println!("  pattern string  : {}", text);
                    println!("  commits written : {}", total);
                    println!("  intensity       : {} commits/pixel (Level 4 neon green)", intensity);
                    println!("  target ref      : refs/heads/{}", target_branch);
                    if push {
                        let _ = push_to_origin(&target_branch);
                    }
                }
                Err(e) => {
                    eprintln!("error: heatmap rasterization failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Clean { branch, push }) => {
            let target_branch = branch.unwrap_or_else(get_current_branch);
            let (name, email) = get_git_config_user();

            println!("git-flex: resetting refs/heads/{} to single clean root commit...", target_branch);
            match painter::clean_branch(&target_branch, &name, &email) {
                Ok(hash) => {
                    println!("  cleaned commit : {}", hash);
                    println!("  target ref     : refs/heads/{}", target_branch);
                    println!("  status         : branch reset to pristine tree (all files preserved)");
                    if push {
                        let _ = push_to_origin(&target_branch);
                    }
                }
                Err(e) => {
                    eprintln!("error: branch clean failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Ai { commit, push }) => {
            let branch = get_current_branch();
            println!("git-flex: generating contributor profile commits & stamping co-author trailers...");

            match ai_squad::inject_contributor_commits(&branch) {
                Ok(count) => {
                    let new_hash = ai_squad::stamp_ai_squad(&commit).unwrap_or_else(|_| "HEAD".to_string());
                    println!("  contributors   : {} verified AI commits generated for refs/heads/{}", count, branch);
                    println!("  stamped commit : {}", new_hash);
                    println!("  models         : {} models added to repository contributors", ai_squad::AI_COAUTHORS.len());
                    if push {
                        let _ = push_to_origin(&branch);
                    }
                }
                Err(e) => {
                    eprintln!("error: contributor injection failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Contributors { count, branch, push }) => {
            let target_branch = branch.unwrap_or_else(get_current_branch);
            println!("git-flex: resolving {} verified GitHub contributors...", count);

            let contributors_list = contributors::fetch_real_github_contributors(count);
            println!("git-flex: injecting {} contributors into refs/heads/{}...", contributors_list.len(), target_branch);

            match contributors::inject_contributors(&target_branch, &contributors_list) {
                Ok(total) => {
                    println!("  injected commits : {}", total);
                    println!("  unique authors   : {}", contributors_list.len());
                    println!("  target ref       : refs/heads/{}", target_branch);
                    if push {
                        let _ = push_to_origin(&target_branch);
                    }
                }
                Err(e) => {
                    eprintln!("error: contributor injection failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Streak { start_year, commits_per_day, branch, push }) => {
            let target_branch = branch.unwrap_or_else(get_current_branch);
            let (name, email) = get_git_config_user();

            println!(
                "git-flex: generating unbroken streak from year {} ({} commit/day) on refs/heads/{}...",
                start_year, commits_per_day, target_branch
            );

            match streak::generate_streak(&target_branch, start_year, commits_per_day, &name, &email) {
                Ok(res) => {
                    println!("  total days     : {}", res.days);
                    println!("  total commits  : {}", res.commits);
                    println!("  throughput     : {:.0} commits/sec ({:.2}s)", res.commits_per_sec, res.elapsed);
                    println!("  target ref     : refs/heads/{}", target_branch);
                    println!("  streak status  : unbroken daily coverage (1969/1970 -> today)");
                    if push {
                        let _ = push_to_origin(&target_branch);
                    }
                }
                Err(e) => {
                    eprintln!("error: streak generation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
