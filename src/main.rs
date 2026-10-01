mod models;
mod monitor;
mod session;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use monitor::Monitor;
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Local Codex CLI timing and token dashboard (no proxy required)"
)]
struct Args {
    /// Codex home containing sessions/ and archived_sessions/.
    #[arg(long, env = "CODEX_HOME")]
    codex_home: Option<PathBuf>,
    /// Maximum session files to load; 0 loads all supported logs.
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(0..=10000))]
    limit: u32,
    /// Include turns finished in the last N hours; 0 includes all loaded history.
    #[arg(long, default_value_t = 24, value_parser = clap::value_parser!(u32).range(0..=8760))]
    hours: u32,
    /// Print one JSON snapshot instead of opening the terminal dashboard.
    #[arg(long)]
    json: bool,
    /// Show synthetic sessions without reading Codex files.
    #[arg(long)]
    demo: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let home = args
        .codex_home
        .or_else(|| dirs::home_dir().map(|p| p.join(".codex")))
        .context("Cannot locate home directory; pass --codex-home")?;
    let mut monitor = Monitor::new(home.clone(), args.limit as usize);
    if args.demo {
        monitor.demo();
    } else {
        if !args.json {
            eprintln!("Loading Codex logs from {} …", home.display());
        }
        monitor.refresh()?;
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&monitor.snapshot(args.hours))?
        );
        return Ok(());
    }
    // Periodic discovery also handles dropped events and a home created later.
    let (tx, rx) = mpsc::sync_channel(1);
    let watched_home = home.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = &event
            && matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            )
            && event.paths.iter().any(|p| {
                p.starts_with(watched_home.join("sessions"))
                    || p.starts_with(watched_home.join("archived_sessions"))
            })
        {
            // Coalesce bursts and ignore read-access events from our own reader.
            let _ = tx.try_send(());
        }
    })?;
    if !args.demo
        && home.exists()
        && let Err(error) = watcher.watch(&home, RecursiveMode::Recursive)
    {
        monitor.warning = Some(format!("File watcher unavailable; polling: {error}"));
    }
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut monitor, &rx, args.demo, args.hours);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    monitor: &mut Monitor,
    rx: &mpsc::Receiver<()>,
    demo: bool,
    mut hours: u32,
) -> Result<()> {
    let mut selected = 0;
    let mut show_charts = true;
    let mut selected_model: Option<String> = None;
    let mut turn_offset: usize = 0;
    let mut refreshed = Instant::now();
    loop {
        let changed = rx.try_recv().is_ok();
        if !demo && (changed || refreshed.elapsed() >= Duration::from_secs(2)) {
            if let Err(error) = monitor.refresh() {
                monitor.warning = Some(error.to_string());
            }
            refreshed = Instant::now();
        }
        let models = monitor.models(hours);
        if let Some(index) = selected_model
            .as_ref()
            .and_then(|name| models.iter().position(|m| &m.model == name))
        {
            selected = index;
        }
        selected = selected.min(models.len().saturating_sub(1));
        turn_offset = turn_offset.min(
            models
                .get(selected)
                .map_or(0, |m| m.turns.len().saturating_sub(1)),
        );
        terminal.draw(|frame| {
            ui::draw(
                frame,
                monitor,
                &models,
                selected,
                hours,
                turn_offset,
                show_charts,
            )
        })?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1).min(models.len().saturating_sub(1));
                    turn_offset = 0;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = selected.saturating_sub(1);
                    turn_offset = 0;
                }
                KeyCode::PageDown => turn_offset = turn_offset.saturating_add(10),
                KeyCode::PageUp => turn_offset = turn_offset.saturating_sub(10),
                KeyCode::Home => turn_offset = 0,
                KeyCode::Char('c') => show_charts = !show_charts,
                KeyCode::Char(c @ ('1' | '2' | '3' | '4')) => {
                    hours = match c {
                        '1' => 1,
                        '2' => 24,
                        '3' => 168,
                        _ => 0,
                    };
                    turn_offset = 0;
                }
                KeyCode::Char('r') if !demo => {
                    selected_model = models.get(selected).map(|m| m.model.clone());
                    drop(models);
                    monitor.refresh()?;
                    continue;
                }
                _ => {}
            }
            selected_model = models.get(selected).map(|m| m.model.clone());
        }
    }
    Ok(())
}
