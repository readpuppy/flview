use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};

use crate::{fsutil, hex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewMode {
    Text,
    Hex,
}

#[derive(Debug)]
struct AppState {
    root: PathBuf,
    cwd: PathBuf,
    entries: Vec<fsutil::EntryInfo>,
    selected: usize,

    preview_mode: PreviewMode,
    preview_path: Option<PathBuf>,
    preview_bytes: Vec<u8>,
    preview_scroll: usize,
    preview_hscroll: usize,

    status: String,
}

pub fn run(root: PathBuf) -> Result<()> {
    let root = fsutil::canonicalize_root(&root)?;

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, root);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, root: PathBuf) -> Result<()> {
    let mut state = AppState {
        cwd: root.clone(),
        root,
        entries: Vec::new(),
        selected: 0,
        preview_mode: PreviewMode::Text,
        preview_path: None,
        preview_bytes: Vec::new(),
        preview_scroll: 0,
        preview_hscroll: 0,
        status: String::new(),
    };

    reload_dir(&mut state)?;

    loop {
        terminal.draw(|f| ui(f, &state))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(&mut state, key)? {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn reload_dir(state: &mut AppState) -> Result<()> {
    state.entries = fsutil::list_dir(&state.cwd)?;
    if state.selected >= state.entries.len() {
        state.selected = state.entries.len().saturating_sub(1);
    }
    state.status = format!("{} items", state.entries.len());
    Ok(())
}

fn open_selected(state: &mut AppState) -> Result<()> {
    if state.entries.is_empty() {
        return Ok(());
    }

    let entry = &state.entries[state.selected];
    if entry.is_dir {
        state.cwd = entry.path.clone();
        state.selected = 0;
        state.preview_path = None;
        state.preview_bytes.clear();
        state.preview_scroll = 0;
        state.preview_hscroll = 0;
        reload_dir(state)?;
        return Ok(());
    }

    // file preview
    state.preview_path = Some(entry.path.clone());
    state.preview_scroll = 0;
    state.preview_hscroll = 0;
    state.preview_bytes = fsutil::read_file_limited(&entry.path, 512 * 1024)
        .with_context(|| format!("Failed to read file: {}", entry.path.display()))?;
    state.status = format!(
        "Previewing: {} ({} bytes)",
        entry.name.to_string_lossy(),
        state.preview_bytes.len()
    );
    Ok(())
}

fn go_up(state: &mut AppState) -> Result<()> {
    if state.cwd == state.root {
        return Ok(());
    }
    if let Some(parent) = state.cwd.parent().map(|p| p.to_path_buf()) {
        state.cwd = parent;
        state.selected = 0;
        state.preview_path = None;
        state.preview_bytes.clear();
        state.preview_scroll = 0;
        state.preview_hscroll = 0;
        reload_dir(state)?;
    }
    Ok(())
}

fn handle_key(state: &mut AppState, key: KeyEvent) -> Result<bool> {
    if key.kind != KeyEventKind::Press {
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Up => {
            if state.selected > 0 {
                state.selected -= 1;
            }
        }
        KeyCode::Down => {
            if state.selected + 1 < state.entries.len() {
                state.selected += 1;
            }
        }
        KeyCode::Char('k') => {
            if state.preview_path.is_some() {
                state.preview_scroll = state.preview_scroll.saturating_sub(1);
            }
        }
        KeyCode::Char('j') => {
            if state.preview_path.is_some() {
                state.preview_scroll = state.preview_scroll.saturating_add(1);
            }
        }
        KeyCode::Char('h') => {
            if state.preview_path.is_some() && state.preview_mode == PreviewMode::Text {
                state.preview_hscroll = state.preview_hscroll.saturating_sub(1);
            } else if state.preview_path.is_none() {
                // Preserve the original "h = hex" behavior when no file is open.
                state.preview_mode = PreviewMode::Hex;
                state.preview_hscroll = 0;
                state.status = "Preview mode: hex".to_string();
            }
        }
        KeyCode::Char('l') => {
            if state.preview_path.is_some() && state.preview_mode == PreviewMode::Text {
                state.preview_hscroll = state.preview_hscroll.saturating_add(1);
            }
        }
        KeyCode::PageUp => {
            state.preview_scroll = state.preview_scroll.saturating_sub(16);
        }
        KeyCode::PageDown => {
            state.preview_scroll = state.preview_scroll.saturating_add(16);
        }
        KeyCode::Enter => {
            open_selected(state)?;
        }
        KeyCode::Backspace => {
            go_up(state)?;
        }
        KeyCode::Char('t') => {
            state.preview_mode = PreviewMode::Text;
            state.preview_hscroll = 0;
            state.status = "Preview mode: text".to_string();
        }
        KeyCode::Char('x') => {
            state.preview_mode = PreviewMode::Hex;
            state.preview_hscroll = 0;
            state.status = "Preview mode: hex".to_string();
        }
        _ => {}
    }

    Ok(false)
}

fn ui(f: &mut ratatui::Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[0]);

    // Left: directory list
    let title = format!("Dir: {}", display_path(&state.root, &state.cwd));
    let items: Vec<ListItem> = state
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let mut name = e.name.to_string_lossy().to_string();
            if e.is_dir {
                name.push('/');
            }
            let marker = if i == state.selected { "> " } else { "  " };
            ListItem::new(Line::from(format!("{}{}", marker, name)))
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(list, main[0]);

    // Right: preview
    let preview_title = match (&state.preview_path, state.preview_mode) {
        (Some(p), PreviewMode::Text) => format!("Text: {}", p.file_name().unwrap_or_default().to_string_lossy()),
        (Some(p), PreviewMode::Hex) => format!("Hex: {}", p.file_name().unwrap_or_default().to_string_lossy()),
        (None, PreviewMode::Text) => "Text".to_string(),
        (None, PreviewMode::Hex) => "Hex".to_string(),
    };

    let preview_text = if state.preview_path.is_none() {
        Text::from("(Press Enter on a file to preview)\nKeys: Up/Down, Enter, Backspace, t, h, x, PgUp/PgDn, q (in preview: j/k scroll, h/l pan)")
    } else {
        match state.preview_mode {
            PreviewMode::Text => {
                let s = String::from_utf8_lossy(&state.preview_bytes).into_owned();
                let lines: Vec<Line> = s
                    .lines()
                    .skip(state.preview_scroll)
                    .take(200)
                    .map(|line| {
                        let clipped: String = line.chars().skip(state.preview_hscroll).collect();
                        Line::from(clipped)
                    })
                    .collect();
                Text::from(lines)
            }
            PreviewMode::Hex => {
                let start = state.preview_scroll * 16;
                let lines = hex::format_hex_lines(&state.preview_bytes, start, 200);
                Text::from(lines.into_iter().map(Line::from).collect::<Vec<_>>())
            }
        }
    };

    let preview = Paragraph::new(preview_text).block(Block::default().borders(Borders::ALL).title(preview_title));
    f.render_widget(preview, main[1]);

    // Bottom status
    let status = Paragraph::new(state.status.clone()).block(Block::default().borders(Borders::TOP));
    f.render_widget(status, chunks[1]);
}

fn display_path(root: &Path, path: &Path) -> String {
    if path == root {
        return "/".to_string();
    }
    match path.strip_prefix(root) {
        Ok(p) => format!("/{}", p.to_string_lossy().replace('\\', "/")),
        Err(_) => path.display().to_string(),
    }
}
