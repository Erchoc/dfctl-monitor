pub mod args;
pub mod data;
pub mod input;
pub mod json_output;
pub mod layout;
pub mod render;
pub mod state;
pub mod stats;
pub mod summary;
pub mod widgets;

pub use args::TraceArgs;

use anyhow::{Context, Result};
use crossterm::event::{self as ct_event, DisableMouseCapture, EnableMouseCapture, Event as CtEvent};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use data::TraceSource;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Terminal;
use state::{TraceAppState, TraceView};
use stats::TraceStats;
use std::io::stdout;
use std::sync::Arc;
use std::time::{Duration, Instant};
use summary::HeuristicSummary;
use tokio::sync::mpsc;

use data::TraceResponse;
use input::Action;

#[derive(thiserror::Error, Debug)]
#[allow(dead_code)] // TraceNotFound used once the HTTP backend can 404
pub enum TraceError {
    #[error("terminal too small: {w}x{h}")]
    TerminalTooSmall { w: u16, h: u16 },
    #[error("trace not found: {0}")]
    TraceNotFound(String),
    #[error("interrupted")]
    Interrupted,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl TraceError {
    pub fn exit_code(&self) -> i32 {
        match self {
            TraceError::TerminalTooSmall { .. } => 2,
            TraceError::TraceNotFound(_) => 3,
            TraceError::Interrupted => 130,
            TraceError::Other(_) => 1,
        }
    }
}

pub fn run(args: TraceArgs) -> Result<()> {
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
            let code = e.downcast_ref::<TraceError>().map(|t| t.exit_code()).unwrap_or(1);
            eprintln!("dfctl trace: {}", e);
            std::process::exit(code);
        }
    }
}

fn run_json(args: TraceArgs) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(async move {
        let source = data::mock::MockTraceSource::default();
        let trace = source.fetch(&args.uuid).await?;
        let stats = TraceStats::compute(&trace, &HeuristicSummary);
        json_output::write_json(&trace, &stats)
    })
}

async fn run_tui(args: TraceArgs) -> Result<()> {
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
    let dir = cache_dir().join("df");
    let _ = std::fs::create_dir_all(&dir);
    let file_appender = tracing_appender::rolling::never(dir, "trace.log");
    let (nb, guard) = tracing_appender::non_blocking(file_appender);
    Box::leak(Box::new(guard));
    let _ = tracing_subscriber::fmt()
        .with_writer(nb)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
    Ok(())
}

fn cache_dir() -> std::path::PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let mut p: std::path::PathBuf = home.into();
        p.push(".cache");
        return p;
    }
    std::path::PathBuf::from(".")
}

async fn main_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    args: TraceArgs,
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let size = terminal.size()?;
    let mut st = TraceAppState::new(args.clone(), (size.width, size.height));

    let source = Arc::new(data::mock::MockTraceSource::default());
    let (tx, mut rx) = mpsc::unbounded_channel::<FetchResult>();

    let (key_tx, mut key_rx) = mpsc::unbounded_channel::<CtEvent>();
    std::thread::Builder::new()
        .name("dfctl-trace-input".into())
        .spawn(move || {
            while let Ok(evt) = ct_event::read() {
                if key_tx.send(evt).is_err() {
                    break;
                }
            }
        })?;

    spawn_fetch(source.clone(), st.args.uuid.clone(), tx.clone());
    st.fetch_in_flight = true;

    let mut tick = tokio::time::interval(Duration::from_millis(33));
    let mut second_tick = tokio::time::interval(Duration::from_secs(1));

    loop {
        while let Ok(res) = rx.try_recv() {
            st.fetch_in_flight = false;
            match res {
                FetchResult::Ok(trace) => {
                    let stats = TraceStats::compute(&trace, &HeuristicSummary);
                    // jump to first error if requested and not yet positioned
                    if st.args.errors && st.last_fetch.is_none() {
                        if let Some(first) = stats.error_spans.first() {
                            let visible = stats.visible_order(&st.collapsed);
                            if let Some(pos) = visible.iter().position(|(id, _, _)| id == first) {
                                st.selected = pos;
                            }
                        }
                    }
                    st.data = Some(trace);
                    st.stats = Some(stats);
                    st.last_fetch = Some(Instant::now());
                    st.error = None;
                    st.next_refresh_at = if st.watch_enabled && !st.watch_paused {
                        Some(Instant::now() + st.refresh_interval())
                    } else {
                        None
                    };
                }
                FetchResult::Err(e) => st.error = Some(e),
            }
        }

        if st.watch_enabled
            && !st.watch_paused
            && !st.fetch_in_flight
            && st.next_refresh_at.map(|t| Instant::now() >= t).unwrap_or(false)
        {
            spawn_fetch(source.clone(), st.args.uuid.clone(), tx.clone());
            st.fetch_in_flight = true;
        }

        let size = terminal.size()?;
        st.terminal_size = (size.width, size.height);
        terminal.draw(|f| {
            let area = f.area();
            draw(area, f.buffer_mut(), &st);
        })?;

        tokio::select! {
            biased;
            Some(evt) = key_rx.recv() => {
                if let CtEvent::Key(k) = evt {
                    use crossterm::event::KeyEventKind;
                    if matches!(k.kind, KeyEventKind::Release) {
                        continue;
                    }
                    match input::handle_key(k, &mut st) {
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
                        Action::RefreshNow => {
                            if !st.fetch_in_flight {
                                spawn_fetch(source.clone(), st.args.uuid.clone(), tx.clone());
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
    Ok(TraceResponse),
    Err(String),
}

fn spawn_fetch(
    source: Arc<data::mock::MockTraceSource>,
    trace_id: String,
    tx: mpsc::UnboundedSender<FetchResult>,
) {
    tokio::spawn(async move {
        match source.fetch(&trace_id).await {
            Ok(t) => {
                let _ = tx.send(FetchResult::Ok(t));
            }
            Err(e) => {
                let _ = tx.send(FetchResult::Err(format!("{}", e)));
            }
        }
    });
}

fn draw(area: Rect, buf: &mut Buffer, st: &TraceAppState) {
    render::draw(area, buf, st);
}

// Keep TraceView referenced for clippy when feature-gating views later.
#[allow(dead_code)]
fn _view_marker(v: &TraceView) -> bool {
    matches!(v, TraceView::Waterfall)
}
