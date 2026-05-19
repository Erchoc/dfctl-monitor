pub mod args;
pub mod config;
pub mod data;
pub mod input;
pub mod json_output;
pub mod layout;
pub mod render;
pub mod state;
pub mod theme;
pub mod util;
pub mod widgets;

pub use args::MonitorArgs;

use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::event::{self as ct_event, DisableMouseCapture, EnableMouseCapture, Event as CtEvent};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use data::*;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use ratatui::Terminal;
use std::io::stdout;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use input::Action;
use layout::LayoutTier;
use state::{AppState, View};

#[derive(thiserror::Error, Debug)]
#[allow(dead_code)] // future variants for HTTP backend integration
pub enum MonitorError {
    #[error("terminal too small: {w}x{h}")]
    TerminalTooSmall { w: u16, h: u16 },
    #[error("application not found: {0}")]
    AppNotFound(String),
    #[error("interrupted")]
    Interrupted,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl MonitorError {
    pub fn exit_code(&self) -> i32 {
        match self {
            MonitorError::TerminalTooSmall { .. } => 2,
            MonitorError::AppNotFound(_) => 3,
            MonitorError::Interrupted => 130,
            MonitorError::Other(_) => 1,
        }
    }
}

pub fn run(mut args: MonitorArgs) -> Result<()> {
    let cfg = config::load();
    if let Some(real) = cfg.monitor.aliases.get(&args.app) {
        args.app = real.clone();
    }
    let result = if args.json {
        run_json(args)
    } else {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()?;
        rt.block_on(run_tui(args))
    };
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let code = e.downcast_ref::<MonitorError>().map(|m| m.exit_code()).unwrap_or(1);
            eprintln!("df monitor: {}", e);
            std::process::exit(code);
        }
    }
}

fn run_json(args: MonitorArgs) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(async move {
        let source = data::mock::MockDataSource::default();
        let now = Utc::now();
        let dur = chrono::Duration::from_std(args.since.into()).unwrap();
        let q = MonitorQuery {
            app: args.app.clone(),
            from: now - dur,
            to: now,
            pods: if args.pod.is_empty() { None } else { Some(args.pod.clone()) },
            metrics: None,
        };
        let resp = source.fetch(q).await?;
        json_output::write_json(&resp)
    })
}

async fn run_tui(args: MonitorArgs) -> Result<()> {
    setup_logging()?;
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout_h = stdout();
    execute!(stdout_h, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout_h);
    let mut terminal = Terminal::new(backend)?;
    let result = main_loop(&mut terminal, args).await;
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture).ok();
    terminal.show_cursor().ok();
    result
}

fn setup_logging() -> Result<()> {
    let dir = dirs_cache_dir().join("df");
    let _ = std::fs::create_dir_all(&dir);
    let file_appender = tracing_appender::rolling::never(dir, "monitor.log");
    let (nb, guard) = tracing_appender::non_blocking(file_appender);
    Box::leak(Box::new(guard));
    tracing_subscriber::fmt()
        .with_writer(nb)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    Ok(())
}

fn dirs_cache_dir() -> std::path::PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let mut p: std::path::PathBuf = home.into();
        p.push(".cache");
        return p;
    }
    std::path::PathBuf::from(".")
}

async fn main_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    args: MonitorArgs,
) -> Result<()> {
    let size = terminal.size()?;
    let mut st = AppState::new(args.clone(), (size.width, size.height));

    let source = Arc::new(data::mock::MockDataSource::default());
    let (tx, mut rx) = mpsc::unbounded_channel::<FetchResult>();

    // Dedicated input thread: `crossterm::event::read` is a blocking call, and
    // we want zero latency between keystroke and state update. Earlier we used
    // `tokio::task::spawn_blocking` per loop iteration which added ~100 ms of
    // scheduling overhead — user perception was "needed two taps". A plain
    // std::thread pushing into a tokio channel is both simpler and faster.
    let (key_tx, mut key_rx) = mpsc::unbounded_channel::<CtEvent>();
    std::thread::Builder::new()
        .name("dfctl-input".into())
        .spawn(move || loop {
            match ct_event::read() {
                Ok(evt) => {
                    if key_tx.send(evt).is_err() {
                        break; // main loop dropped the receiver — shutting down
                    }
                }
                Err(_) => break,
            }
        })?;

    spawn_fetch(source.clone(), st.clone_for_query(), tx.clone());
    st.fetch_in_flight = true;

    let mut tick = tokio::time::interval(Duration::from_millis(33));
    let mut second_tick = tokio::time::interval(Duration::from_secs(1));

    loop {
        while let Ok(res) = rx.try_recv() {
            st.fetch_in_flight = false;
            match res {
                FetchResult::Ok(resp) => {
                    st.data = Some(resp);
                    st.last_fetch = Some(Instant::now());
                    st.error = None;
                    if st.watch_enabled && !st.watch_paused {
                        st.next_refresh_at = Some(Instant::now() + st.refresh_interval());
                    } else {
                        st.next_refresh_at = None;
                    }
                }
                FetchResult::Err(e) => st.error = Some(e),
            }
        }

        if st.watch_enabled
            && !st.watch_paused
            && !st.fetch_in_flight
            && st.next_refresh_at.map(|t| Instant::now() >= t).unwrap_or(false)
        {
            spawn_fetch(source.clone(), st.clone_for_query(), tx.clone());
            st.fetch_in_flight = true;
        }

        let size = terminal.size()?;
        st.terminal_size = (size.width, size.height);
        terminal.draw(|f| {
            let area = f.area();
            draw(area, f.buffer_mut(), &st);
        })?;

        tokio::select! {
            // biased: prefer keystrokes over redraw ticks so input never has to
            // wait for the next 33 ms frame to be processed.
            biased;
            Some(evt) = key_rx.recv() => {
                if let CtEvent::Key(k) = evt {
                    // crossterm sends Press, Release and Repeat events when the host
                    // terminal speaks kitty keyboard protocol (Termius, WezTerm, recent
                    // iTerm). Accept Press *and* Repeat so holding an arrow key still
                    // pages continuously; drop Release which was the source of the
                    // "needs two taps" bug — every key was firing twice without this.
                    use crossterm::event::KeyEventKind;
                    if matches!(k.kind, KeyEventKind::Release) {
                        continue;
                    }
                    let tier = LayoutTier::from_size(st.terminal_size.0, st.terminal_size.1, &st.args);
                    let action = input::handle_key(k, &mut st, tier);
                    match action {
                        Action::Quit => break,
                        Action::ToggleWatch => {
                            st.watch_enabled = !st.watch_enabled;
                            st.watch_paused = false;
                            st.next_refresh_at = if st.watch_enabled {
                                Some(Instant::now() + st.refresh_interval())
                            } else {
                                None
                            };
                        }
                        Action::TogglePause => {
                            if st.watch_enabled {
                                st.watch_paused = !st.watch_paused;
                                if !st.watch_paused {
                                    st.next_refresh_at = Some(Instant::now() + st.refresh_interval());
                                }
                            }
                        }
                        Action::RefreshNow | Action::RangeChanged => {
                            if !st.fetch_in_flight {
                                spawn_fetch(source.clone(), st.clone_for_query(), tx.clone());
                                st.fetch_in_flight = true;
                            }
                        }
                        Action::None => {}
                    }
                }
            }
            _ = tick.tick() => {}
            _ = second_tick.tick() => {}
        }
    }
    Ok(())
}

enum FetchResult {
    Ok(MonitorResponse),
    Err(String),
}

fn spawn_fetch(
    source: Arc<data::mock::MockDataSource>,
    query: MonitorQuery,
    tx: mpsc::UnboundedSender<FetchResult>,
) {
    tokio::spawn(async move {
        match source.fetch(query).await {
            Ok(r) => {
                let _ = tx.send(FetchResult::Ok(r));
            }
            Err(e) => {
                let _ = tx.send(FetchResult::Err(format!("{}", e)));
            }
        }
    });
}

impl AppState {
    fn clone_for_query(&self) -> MonitorQuery {
        let now = Utc::now();
        let dur = chrono::Duration::from_std(self.args.since.into())
            .unwrap_or(chrono::Duration::hours(3));
        let from = self.args.from.map(|t| t.with_timezone(&Utc)).unwrap_or(now - dur);
        let to = self.args.to.map(|t| t.with_timezone(&Utc)).unwrap_or(now);
        let pods = if self.args.pod.is_empty() {
            None
        } else {
            Some(self.args.pod.clone())
        };
        MonitorQuery {
            app: self.args.app.clone(),
            from,
            to,
            pods,
            metrics: None,
        }
    }
}

/// Top-level view router. Picks the right draw_* function based on (view, tier).
fn draw(area: Rect, buf: &mut Buffer, st: &AppState) {
    let tier = LayoutTier::from_size(area.width, area.height, &st.args);
    render::paint_bg(buf, area);

    match (st.view.clone(), tier) {
        (_, LayoutTier::TooSmall) => {
            widgets::too_small::TooSmall {
                w: area.width,
                h: area.height,
            }
            .render(area, buf);
        }
        (View::Help, _) => {
            draw_backdrop(area, buf, st, tier);
            widgets::help::HelpOverlay.render(area, buf);
        }
        (View::RangePicker { selected, previous }, _) => {
            match previous.as_ref() {
                View::SingleMetric(m) if matches!(tier, LayoutTier::Phone) => {
                    render::draw_single_phone(area, buf, st, *m)
                }
                View::SingleMetric(m) => render::draw_single(area, buf, st, *m),
                _ => draw_backdrop(area, buf, st, tier),
            }
            widgets::range_picker::RangePicker { selected }.render(area, buf);
        }
        (View::SingleMetric(m), LayoutTier::Phone) => render::draw_single_phone(area, buf, st, m),
        (View::SingleMetric(m), _) => render::draw_single(area, buf, st, m),
        (_, LayoutTier::Phone) => render::draw_phone(area, buf, st),
        _ => render::draw_overview(area, buf, st, tier),
    }
}

fn draw_backdrop(area: Rect, buf: &mut Buffer, st: &AppState, tier: LayoutTier) {
    if matches!(tier, LayoutTier::Phone) {
        render::draw_phone(area, buf, st);
    } else {
        render::draw_overview(area, buf, st, tier);
    }
}
