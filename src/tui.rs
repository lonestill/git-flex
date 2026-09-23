use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};
use std::io;
use std::process::Command;

use crate::blaster::blast_commits;
use crate::contributors::{fetch_real_github_contributors, inject_contributors, LEGENDS};
use crate::painter::{clean_branch, paint_heatmap, render_text_to_grid};
use crate::vanity::mine_vanity;

pub fn run_tui() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = tui_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("TUI Error: {:?}", err);
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PainterField {
    Text,
    Year,
}

struct AppState {
    tab_index: usize,
    vanity_input: String,
    blast_input: String,
    painter_input: String,
    painter_year_input: String,
    painter_field: PainterField,
    painter_fill_bg: bool,
    contributors_input: String,
    status_msg: String,
    current_branch: String,
    head_commit: String,
    remote: Option<String>,
    is_git: bool,
    has_commits: bool,
    auto_push: bool,
}

fn check_git_state() -> (bool, bool, String, String, Option<String>) {
    let is_git = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_git {
        return (false, false, "NO_GIT".to_string(), "NONE".to_string(), None);
    }

    let branch = Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            } else {
                None
            }
        })
        .unwrap_or_else(|| "detached".to_string());

    let head_out = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    let (has_commits, head) = match head_out {
        Ok(o) if o.status.success() => (true, String::from_utf8_lossy(&o.stdout).trim().to_string()),
        _ => (false, "EMPTY".to_string()),
    };

    let remote = get_git_remote();

    (true, has_commits, branch, head, remote)
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

fn execute_push(branch: &str) -> Result<String, String> {
    crate::push_to_origin(branch)
}

fn tui_loop<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let (is_git, has_commits, branch, head, remote) = check_git_state();

    let initial_status = if !is_git {
        "⚠️ NOT A GIT REPOSITORY! Press [i] to run 'git init' in this directory.".to_string()
    } else if !has_commits {
        "⚠️ REPOSITORY HAS NO COMMITS! Press [c] to create initial commit.".to_string()
    } else if remote.is_none() {
        "Ready. Notice: No remote configured. Auto-Push is OFF.".to_string()
    } else {
        "Ready. Use [Tab] to switch. Press [Enter] to run. Auto-Push is OFF.".to_string()
    };

    let mut state = AppState {
        tab_index: 0,
        vanity_input: "000000".to_string(),
        blast_input: "1000".to_string(),
        painter_input: "LZT".to_string(),
        painter_year_input: String::new(),
        painter_field: PainterField::Text,
        painter_fill_bg: true,
        contributors_input: "25".to_string(),
        status_msg: initial_status,
        current_branch: branch,
        head_commit: head,
        remote,
        is_git,
        has_commits,
        auto_push: false,
    };

    loop {
        terminal.draw(|f| ui(f, &state))?;

        if let Event::Key(key) = event::read()? {
            // 1. First check Control key combinations
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('c') | KeyCode::Char('q') => break,
                    KeyCode::Char('p') => {
                        state.auto_push = !state.auto_push;
                        state.status_msg = if state.auto_push {
                            "Auto-Push to remote enabled [ON].".to_string()
                        } else {
                            "Auto-Push to remote disabled [OFF].".to_string()
                        };
                        continue;
                    }
                    KeyCode::Char('b') => {
                        state.painter_fill_bg = !state.painter_fill_bg;
                        state.status_msg = if state.painter_fill_bg {
                            "Heatmap Background Fill: [ON] (1 commit/day for Level-1 base)".to_string()
                        } else {
                            "Heatmap Background Fill: [OFF] (empty background)".to_string()
                        };
                        continue;
                    }
                    KeyCode::Char('x') => {
                        if !state.is_git {
                            state.status_msg = "ERROR: Not a git repo!".to_string();
                            continue;
                        }
                        let (name, email) = crate::get_git_config_user();
                        match clean_branch(&state.current_branch, &name, &email) {
                            Ok(hash) => {
                                let (_, _, _, hd, rem) = check_git_state();
                                state.head_commit = hd;
                                state.remote = rem;
                                state.status_msg = format!("CLEANED: Branch reset to clean commit {}", &hash[..7.min(hash.len())]);
                                if state.auto_push {
                                    let _ = execute_push(&state.current_branch);
                                }
                            }
                            Err(e) => state.status_msg = format!("ERROR: {}", e),
                        }
                        continue;
                    }
                    KeyCode::Char('1') => { state.tab_index = 0; continue; }
                    KeyCode::Char('2') => { state.tab_index = 1; continue; }
                    KeyCode::Char('3') => { state.tab_index = 2; continue; }
                    KeyCode::Char('4') => { state.tab_index = 3; continue; }
                    _ => {}
                }
            }

            // 2. Navigation and non-character commands
            match key.code {
                KeyCode::Esc => break,
                KeyCode::F(5) => {
                    state.auto_push = !state.auto_push;
                    state.status_msg = if state.auto_push {
                        "Auto-Push to remote enabled [ON].".to_string()
                    } else {
                        "Auto-Push to remote disabled [OFF].".to_string()
                    };
                }
                KeyCode::F(6) => {
                    state.painter_fill_bg = !state.painter_fill_bg;
                    state.status_msg = if state.painter_fill_bg {
                        "Heatmap Background Fill: [ON] (1 commit/day for Level-1 base)".to_string()
                    } else {
                        "Heatmap Background Fill: [OFF] (empty background)".to_string()
                    };
                }
                KeyCode::F(7) => {
                    if state.is_git {
                        let (name, email) = crate::get_git_config_user();
                        match clean_branch(&state.current_branch, &name, &email) {
                            Ok(hash) => {
                                let (_, _, _, hd, rem) = check_git_state();
                                state.head_commit = hd;
                                state.remote = rem;
                                state.status_msg = format!("CLEANED: Branch reset to clean commit {}", &hash[..7.min(hash.len())]);
                                if state.auto_push {
                                    let _ = execute_push(&state.current_branch);
                                }
                            }
                            Err(e) => state.status_msg = format!("ERROR: {}", e),
                        }
                    }
                }
                KeyCode::F(1) => state.tab_index = 0,
                KeyCode::F(2) => state.tab_index = 1,
                KeyCode::F(3) => state.tab_index = 2,
                KeyCode::F(4) => state.tab_index = 3,
                KeyCode::Tab => {
                    state.tab_index = (state.tab_index + 1) % 4;
                }
                KeyCode::BackTab => {
                    state.tab_index = if state.tab_index == 0 { 3 } else { state.tab_index - 1 };
                }
                KeyCode::Char('i') | KeyCode::Char('I') if !state.is_git => {
                    let _ = Command::new("git").arg("init").status();
                    let (is_g, has_c, br, hd, rem) = check_git_state();
                    state.is_git = is_g;
                    state.has_commits = has_c;
                    state.current_branch = br;
                    state.head_commit = hd;
                    state.remote = rem;
                    state.status_msg = "Git initialized! Press [c] to create initial commit.".to_string();
                }
                KeyCode::Char('c') | KeyCode::Char('C') if state.is_git && !state.has_commits => {
                    let _ = Command::new("git").args(["add", "."]).status();
                    let _ = Command::new("git").args(["commit", "-m", "chore: initial commit", "--allow-empty"]).status();
                    let (is_g, has_c, br, hd, rem) = check_git_state();
                    state.is_git = is_g;
                    state.has_commits = has_c;
                    state.current_branch = br;
                    state.head_commit = hd;
                    state.remote = rem;
                    state.status_msg = "Initial commit created! Ready to flex.".to_string();
                }
                KeyCode::Up | KeyCode::Down if state.tab_index == 2 => {
                    state.painter_field = match state.painter_field {
                        PainterField::Text => PainterField::Year,
                        PainterField::Year => PainterField::Text,
                    };
                }
                KeyCode::Backspace => match state.tab_index {
                    0 => { state.vanity_input.pop(); }
                    1 => { state.blast_input.pop(); }
                    2 => {
                        if state.painter_field == PainterField::Text {
                            state.painter_input.pop();
                        } else {
                            state.painter_year_input.pop();
                        }
                    }
                    3 => { state.contributors_input.pop(); }
                    _ => {}
                },
                KeyCode::Char(ch) if ch.is_ascii_graphic() || ch == ' ' => match state.tab_index {
                    0 if state.vanity_input.len() < 12 && ch.is_ascii_hexdigit() => {
                        state.vanity_input.push(ch.to_ascii_lowercase());
                    }
                    1 if state.blast_input.len() < 8 && ch.is_ascii_digit() => {
                        state.blast_input.push(ch);
                    }
                    2 if state.painter_field == PainterField::Text && state.painter_input.len() < 8 => {
                        state.painter_input.push(ch.to_ascii_uppercase());
                    }
                    2 if state.painter_field == PainterField::Year && state.painter_year_input.len() < 4 && ch.is_ascii_digit() => {
                        state.painter_year_input.push(ch);
                    }
                    3 if state.contributors_input.len() < 5 && ch.is_ascii_digit() => {
                        state.contributors_input.push(ch);
                    }
                    _ => {}
                },
                KeyCode::Enter => {
                    if !state.is_git {
                        state.status_msg = "ERROR: Not a git repo! Press [i] to git init.".to_string();
                        continue;
                    }

                    state.status_msg = "Executing operation...".to_string();
                    terminal.draw(|f| ui(f, &state))?;

                    let mut push_needed = false;

                    match state.tab_index {
                        0 => {
                            if !state.has_commits {
                                state.status_msg = "ERROR: No commits in repo. Press [c] first.".to_string();
                                continue;
                            }
                            match mine_vanity(&state.vanity_input, "HEAD", false, false) {
                                Ok(res) => {
                                    state.head_commit = res.new_hash[..7].to_string();
                                    state.status_msg = format!(
                                        "MINED: {} in {:.2}s ({:.1} MH/s)",
                                        res.new_hash, res.elapsed, res.hashrate
                                    );
                                    push_needed = true;
                                }
                                Err(e) => state.status_msg = format!("ERROR: {}", e),
                            }
                        }
                        1 => {
                            let count = state.blast_input.parse::<usize>().unwrap_or(100);
                            let (name, email) = crate::get_git_config_user();
                            match blast_commits(&state.current_branch, count, "chore: blast commit", &name, &email) {
                                Ok(res) => {
                                    state.has_commits = true;
                                    let (_, _, _, hd, rem) = check_git_state();
                                    state.head_commit = hd;
                                    state.remote = rem;
                                    state.status_msg = format!(
                                        "BLASTED: {} commits in {:.2}s ({:.0} commits/sec)",
                                        res.count, res.elapsed, res.commits_per_sec
                                    );
                                    push_needed = true;
                                }
                                Err(e) => state.status_msg = format!("ERROR: {}", e),
                            }
                        }
                        2 => {
                            let (name, email) = crate::get_git_config_user();
                            let year_opt = if state.painter_year_input.trim().is_empty() {
                                None
                            } else {
                                state.painter_year_input.trim().parse::<i32>().ok()
                            };
                            match paint_heatmap(&state.current_branch, &state.painter_input, 50, year_opt, state.painter_fill_bg, false, &name, &email) {
                                Ok(total) => {
                                    state.has_commits = true;
                                    let (_, _, _, hd, rem) = check_git_state();
                                    state.head_commit = hd;
                                    state.remote = rem;
                                    let yr_str = match year_opt {
                                        Some(y) => format!("{}", y),
                                        None => "rolling 52w".to_string(),
                                    };
                                    state.status_msg = format!(
                                        "PAINTED: Generated {} commits for '{}' ({})",
                                        total, state.painter_input, yr_str
                                    );
                                    push_needed = true;
                                }
                                Err(e) => state.status_msg = format!("ERROR: {}", e),
                            }
                        }
                        3 => {
                            let count = state.contributors_input.parse::<usize>().unwrap_or(25);
                            state.status_msg = format!("Resolving {} verified GitHub contributors...", count);
                            terminal.draw(|f| ui(f, &state))?;

                            let list = fetch_real_github_contributors(count);
                            match inject_contributors(&state.current_branch, &list) {
                                Ok(total) => {
                                    state.has_commits = true;
                                    let (_, _, _, hd, rem) = check_git_state();
                                    state.head_commit = hd;
                                    state.remote = rem;
                                    state.status_msg = format!(
                                        "CONTRIBUTORS: Injected {} verified authors into {}",
                                        total, state.current_branch
                                    );
                                    push_needed = true;
                                }
                                Err(e) => state.status_msg = format!("ERROR: {}", e),
                            }
                        }
                        _ => {}
                    }

                    if push_needed && state.auto_push {
                        state.status_msg.push_str(" | Pushing to remote...");
                        terminal.draw(|f| ui(f, &state))?;

                        match execute_push(&state.current_branch) {
                            Ok(msg) => {
                                state.status_msg.push_str(&format!(" | ✅ {}", msg));
                            }
                            Err(err) => {
                                state.status_msg.push_str(&format!(" | ⚠️ {}", err));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(f.area());

    // 1. Header Banner
    let branch_badge = if !state.is_git {
        Span::styled(" [no git] ", Style::default().fg(Color::Red))
    } else {
        Span::styled(format!(" {} ", state.current_branch), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    };

    let head_badge = if !state.has_commits {
        Span::styled(" [empty] ", Style::default().fg(Color::Yellow))
    } else {
        Span::styled(format!(" {} ", state.head_commit), Style::default().fg(Color::Cyan))
    };

    let remote_badge = match state.remote.as_deref() {
        Some(r) => Span::styled(format!(" {} ", r), Style::default().fg(Color::Cyan)),
        None => Span::styled(" [none] ", Style::default().fg(Color::Red)),
    };

    let push_badge = if state.auto_push {
        Span::styled(" [on] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" [off] ", Style::default().fg(Color::DarkGray))
    };

    let header_line = Line::from(vec![
        Span::styled(" git-flex ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" │ branch: ", Style::default().fg(Color::DarkGray)),
        branch_badge,
        Span::styled("│ head: ", Style::default().fg(Color::DarkGray)),
        head_badge,
        Span::styled("│ remote: ", Style::default().fg(Color::DarkGray)),
        remote_badge,
        Span::styled("│ auto-push: ", Style::default().fg(Color::DarkGray)),
        push_badge,
        Span::styled(" (Ctrl+P / F5)", Style::default().fg(Color::DarkGray)),
    ]);

    let header = Paragraph::new(header_line)
        .block(Block::default().borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded).style(Style::default().fg(Color::DarkGray)));
    f.render_widget(header, chunks[0]);

    // 2. Tabs
    let titles = vec![
        Line::from(Span::raw(" 1: Vanity Hash ")),
        Line::from(Span::raw(" 2: Commit Batch ")),
        Line::from(Span::raw(" 3: Heatmap Matrix ")),
        Line::from(Span::raw(" 4: Contributors ")),
    ];
    let tabs = Tabs::new(titles)
        .select(state.tab_index)
        .block(Block::default().borders(Borders::BOTTOM).border_type(ratatui::widgets::BorderType::Plain))
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[1]);

    // 3. Tab Body
    match state.tab_index {
        0 => render_vanity_tab(f, chunks[2], state),
        1 => render_blaster_tab(f, chunks[2], state),
        2 => render_painter_tab(f, chunks[2], state),
        3 => render_contributors_tab(f, chunks[2], state),
        _ => {}
    }

    // 4. Status Bar
    let status_style = if state.status_msg.contains("ERROR") || state.status_msg.contains("NOT A GIT") {
        Style::default().fg(Color::Red)
    } else if state.status_msg.contains("MINED") || state.status_msg.contains("BLASTED") || state.status_msg.contains("PAINTED") || state.status_msg.contains("STAMPED") {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let hotkeys = if !state.is_git {
        "[i] git init │ [Esc] exit"
    } else if !state.has_commits {
        "[c] initial commit │ [Esc] exit"
    } else if state.tab_index == 2 {
        "[Enter] paint │ [↑/↓] field │ [Ctrl+B/F6] bg │ [Ctrl+X/F7] clean │ [Ctrl+P] push │ [Tab] switch"
    } else {
        "[Tab] next tab │ [Enter] execute │ [Ctrl+P / F5] toggle push │ [Esc] exit"
    };

    let footer = Paragraph::new(format!(" {}  │  {}", state.status_msg, hotkeys))
        .style(status_style)
        .block(Block::default().borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded).title(" Status "));
    f.render_widget(footer, chunks[3]);
}

fn render_vanity_tab(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let block = Block::default()
        .title(" Vanity Hash Miner ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().fg(Color::Cyan));

    let text = vec![
        Line::from("Generates custom SHA-1 commit hashes using multi-threaded midstate precomputation."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Target Prefix   : ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("[ ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.vanity_input, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::default().fg(Color::Cyan)),
            Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            Span::styled("  (hex: 0-9, a-f, e.g. 000000, deadbeef, 1337c0de)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Strategy        : ", Style::default().fg(Color::DarkGray)),
            Span::styled("Invisible whitespace padding (commit message stays clean)", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Parallelization : ", Style::default().fg(Color::DarkGray)),
            Span::styled("Disjoint partition per Rayon worker thread (zero redundant work)", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Press [Enter] to start mining.", Style::default().fg(Color::Cyan))),
    ];

    let p = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn render_blaster_tab(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let block = Block::default()
        .title(" Streaming Commit Batch ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().fg(Color::Cyan));

    let (author_name, author_email) = crate::get_git_config_user();

    let text = vec![
        Line::from("Pipes high-volume commits directly via git fast-import stream pipeline."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Commit Count    : ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("[ ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.blast_input, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::default().fg(Color::Cyan)),
            Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            Span::styled("  (recommended: 1000 - 10000)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Target Branch   : ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.current_branch, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Author Identity : ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} <{}>", author_name, author_email), Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Press [Enter] to stream commits into target branch.", Style::default().fg(Color::Cyan))),
    ];

    let p = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn render_painter_tab(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let block = Block::default()
        .title(" Contribution Heatmap Matrix ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().fg(Color::Cyan));

    let bg_badge = if state.painter_fill_bg {
        Span::styled(" [ON] 1 commit/day for Level-1 uniform base ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" [OFF] unpainted days empty ", Style::default().fg(Color::DarkGray))
    };

    let text_cursor = if state.painter_field == PainterField::Text { "█" } else { " " };
    let year_cursor = if state.painter_field == PainterField::Year { "█" } else { " " };

    let mut lines = vec![
        Line::from("Rasterizes alphanumeric text onto the 7x52 GitHub contribution graph."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Pattern Text    : ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("[ ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.painter_input, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(text_cursor, Style::default().fg(Color::Cyan)),
            Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            Span::styled("  (max ~7 characters, centered in calendar; [↑/↓] switch)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("Target Year     : ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("[ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.painter_year_input.is_empty() { "current" } else { &state.painter_year_input },
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(year_cursor, Style::default().fg(Color::Cyan)),
            Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            Span::styled("  (1969 - 2026, or empty for rolling 52 weeks; [↑/↓] switch)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("Intensity       : ", Style::default().fg(Color::DarkGray)),
            Span::styled("50 commits/pixel ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("(guarantees Level-4 nuclear neon green #39d353)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("Background Fill : ", Style::default().fg(Color::DarkGray)),
            bg_badge,
            Span::styled(" (toggle: Ctrl+B / F6)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("Clean Canvas    : ", Style::default().fg(Color::DarkGray)),
            Span::styled("Press [Ctrl+X / F7] to wipe branch to 1 clean commit", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Calendar Preview (52 weeks x 7 days):", Style::default().fg(Color::DarkGray))),
    ];

    let day_labels = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let grid = render_text_to_grid(&state.painter_input);
    for row in 0..7 {
        let mut row_spans = Vec::new();
        row_spans.push(Span::styled(format!("  {} ", day_labels[row]), Style::default().fg(Color::DarkGray)));
        for col in 0..52 {
            if grid[row][col] {
                row_spans.push(Span::styled("■", Style::default().fg(Color::Green)));
            } else if state.painter_fill_bg {
                row_spans.push(Span::styled("·", Style::default().fg(Color::DarkGray)));
            } else {
                row_spans.push(Span::styled(" ", Style::default().fg(Color::DarkGray)));
            }
        }
        lines.push(Line::from(row_spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Press [Enter] to generate matrix commits.", Style::default().fg(Color::Cyan))));

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn render_contributors_tab(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let block = Block::default()
        .title(" Contributor Inflation Engine ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().fg(Color::Cyan));

    let mut lines = vec![
        Line::from("Injects verified GitHub accounts directly into git tree as primary authors."),
        Line::from("GitHub indexes these commits into the official repository Contributor graph."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Contributor Count : ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("[ ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.contributors_input, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::default().fg(Color::Cyan)),
            Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            Span::styled("  (fetches live via gh API + fallback to curated legends)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Target Branch     : ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.current_branch, Style::default().fg(Color::White)),
            Span::styled(" (must match GitHub default branch to reflect in graphs)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("Identity Mapping  : ", Style::default().fg(Color::DarkGray)),
            Span::styled("<id>+<login>@users.noreply.github.com", Style::default().fg(Color::Green)),
            Span::styled(" (100% verified GitHub profile resolution)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Curated Legends Pool (Sample):", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
    ];

    for (name, _id, login) in LEGENDS.iter().take(6) {
        lines.push(Line::from(vec![
            Span::styled("  • ", Style::default().fg(Color::Cyan)),
            Span::styled(format!("{:<26}", name), Style::default().fg(Color::White)),
            Span::styled(format!("@{}", login), Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Press [Enter] to fetch contributors and inject commits.", Style::default().fg(Color::Cyan))));

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
}
