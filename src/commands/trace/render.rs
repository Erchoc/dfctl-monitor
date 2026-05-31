//! Trace TUI rendering: waterfall, minimap, summary, span detail, phone.
//!
//! Graphics use the shared Braille `Canvas` (`crate::tui::braille`). Animations
//! (intro reveal, error pulse, critical-path glow, the flow comet, and the
//! minimap scan line) are driven by wall-clock time from `AppState`, so they're
//! smooth regardless of redraw cadence.

use super::data::{fmt_dur_us, Span, SpanStatus, TraceResponse, TraceStatus};
use super::layout::TraceTier;
use super::stats::TraceStats;
use super::state::{TraceAppState, TraceView};
use super::widgets;
use crate::tui::braille::Canvas;
use crate::tui::colors::*;
use crate::tui::kpi_card::KpiCard;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span as TSpan};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap};

const REVEAL_SECS: f32 = 0.35;

// ── shared helpers ─────────────────────────────────────────────────────────

pub fn paint_bg(buf: &mut Buffer, area: Rect) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(' ');
                c.set_bg(BG.to_color());
                c.set_fg(BG.to_color());
            }
        }
    }
}

fn span_base_color(s: &Span) -> Rgb {
    if s.is_error() {
        ACCENT_ALERT
    } else {
        pod_color(&s.service)
    }
}

fn status_border(status: TraceStatus) -> Rgb {
    match status {
        TraceStatus::Ok => BORDER_DIM,
        TraceStatus::Partial => ACCENT_WARN,
        TraceStatus::Error => ACCENT_ALERT,
    }
}

fn render_loading(area: Rect, buf: &mut Buffer) {
    if area.width < 20 || area.height == 0 {
        return;
    }
    let r = Rect::new(area.x + area.width / 2 - 9, area.y + area.height / 2, 18, 1);
    Paragraph::new(Line::from(TSpan::styled(
        "⠋ loading trace...",
        Style::default().fg(ACCENT_OK.to_color()),
    )))
    .render(r, buf);
}

fn bar_str(frac: f64, width: usize) -> String {
    let blocks = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let eighths = (frac.clamp(0.0, 1.0) * width as f64 * 8.0).round() as usize;
    let full = eighths / 8;
    let rem = eighths % 8;
    let mut s = String::new();
    for _ in 0..full.min(width) {
        s.push('█');
    }
    if rem > 0 && full < width {
        s.push(blocks[rem - 1]);
    }
    s
}

// ── top-level router ───────────────────────────────────────────────────────

pub fn draw(area: Rect, buf: &mut Buffer, st: &TraceAppState) {
    let tier = TraceTier::from_size(area.width, area.height);
    paint_bg(buf, area);

    if matches!(tier, TraceTier::TooSmall) {
        widgets::TooSmall { w: area.width, h: area.height }.render(area, buf);
        return;
    }

    match &st.view {
        TraceView::Help => {
            backdrop(area, buf, st, tier);
            widgets::HelpOverlay.render(area, buf);
        }
        TraceView::Summary => draw_summary_full(area, buf, st),
        TraceView::SpanDetail(id) => draw_detail(area, buf, st, id),
        TraceView::Waterfall => backdrop(area, buf, st, tier),
    }
}

fn backdrop(area: Rect, buf: &mut Buffer, st: &TraceAppState, tier: TraceTier) {
    if tier.is_phone() {
        draw_phone(area, buf, st);
    } else {
        draw_waterfall(area, buf, st, matches!(tier, TraceTier::WideSidebar));
    }
}

// ── header / footer text ────────────────────────────────────────────────────

fn header_line<'a>(trace: &'a TraceResponse, stats: &TraceStats) -> Line<'a> {
    let errs = stats.error_spans.len();
    let mut spans = vec![
        TSpan::styled(" ◆ ", Style::default().fg(ACCENT_OK.to_color())),
        TSpan::styled(
            format!("dfctl trace {} ", short_id(&trace.trace_id)),
            Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD),
        ),
        TSpan::styled(
            format!("· {} {} ", trace.root_service, trace.root_operation),
            Style::default().fg(TEXT_SECONDARY.to_color()),
        ),
        TSpan::styled(
            format!(
                " {} · {} spans · {} svc ",
                fmt_dur_us(trace.duration_us),
                trace.spans.len(),
                trace.services.len()
            ),
            Style::default().fg(ACCENT_INFO.to_color()),
        ),
    ];
    if errs > 0 {
        spans.push(TSpan::styled(
            format!("◆ {} ERROR ", errs),
            Style::default().fg(ACCENT_ALERT.to_color()).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(TSpan::styled(
            "✓ OK ",
            Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn short_id(id: &str) -> String {
    if id.len() > 12 {
        id[..12].to_string()
    } else {
        id.to_string()
    }
}

fn footer_line<'a>(hint: &'a str) -> Line<'a> {
    Line::from(TSpan::styled(
        hint,
        Style::default().fg(TEXT_DIM.to_color()),
    ))
}

// ── waterfall (overview) ─────────────────────────────────────────────────────

fn draw_waterfall(area: Rect, buf: &mut Buffer, st: &TraceAppState, sidebar: bool) {
    let (trace, stats) = match (st.data.as_ref(), st.stats.as_ref()) {
        (Some(t), Some(s)) => (t, s),
        _ => {
            render_loading(area, buf);
            return;
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(status_border(trace.status).to_color()))
        .title(header_line(trace, stats))
        .title_bottom(footer_line(
            " [↑↓] span  [←→] fold  [⏎] detail  [s] summary  [c] critical  [f] flow  [e] err  [q] quit ",
        ));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width < 20 || inner.height < 6 {
        return;
    }

    // split off a summary sidebar on very wide terminals
    let (main, side) = if sidebar && inner.width > 150 {
        let sw = 40u16;
        (
            Rect::new(inner.x, inner.y, inner.width - sw - 1, inner.height),
            Some(Rect::new(inner.x + inner.width - sw, inner.y, sw, inner.height)),
        )
    } else {
        (inner, None)
    };

    // vertical sub-layout inside `main`
    let mm_h = 2u16;
    let mm = Rect::new(main.x, main.y, main.width, mm_h);
    let colhdr_y = main.y + mm_h;
    let body_y = colhdr_y + 1;
    let status_y = main.y + main.height - 1;
    if status_y <= body_y {
        return;
    }
    let body_h = status_y - body_y;

    // column split
    let tree_w = ((main.width as f32 * 0.42) as u16).clamp(22, 64).min(main.width.saturating_sub(10));
    let gap = 1u16;
    let wf_x = main.x + tree_w + gap;
    let wf_w = main.right().saturating_sub(wf_x);

    draw_minimap(mm, buf, st, trace, stats);
    draw_colheader(colhdr_y, main.x, tree_w, wf_x, wf_w, trace, buf);
    draw_body(
        Rect::new(main.x, body_y, main.width, body_h),
        tree_w,
        wf_x,
        wf_w,
        buf,
        st,
        trace,
        stats,
    );
    draw_status_line(status_y, main.x, main.width, buf, trace, stats);

    if let Some(s) = side {
        draw_summary_panel(s, buf, st, trace, stats, false);
    }
}

fn draw_colheader(
    y: u16,
    left_x: u16,
    tree_w: u16,
    wf_x: u16,
    wf_w: u16,
    trace: &TraceResponse,
    buf: &mut Buffer,
) {
    Paragraph::new(Line::from(TSpan::styled(
        " SPAN TREE",
        Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
    )))
    .render(Rect::new(left_x, y, tree_w, 1), buf);

    let total = fmt_dur_us(trace.duration_us);
    let mid = fmt_dur_us(trace.duration_us / 2);
    let axis = Rect::new(wf_x, y, wf_w, 1);
    Paragraph::new(Line::from(TSpan::styled("0", Style::default().fg(TEXT_DIM.to_color()))))
        .render(axis, buf);
    Paragraph::new(Line::from(TSpan::styled(mid, Style::default().fg(TEXT_DIM.to_color()))))
        .alignment(Alignment::Center)
        .render(axis, buf);
    Paragraph::new(Line::from(TSpan::styled(total, Style::default().fg(TEXT_DIM.to_color()))))
        .alignment(Alignment::Right)
        .render(axis, buf);
}

/// Precompute tree-connector prefixes for the full visible list so the window
/// can draw correct │ ├ └ continuation regardless of scroll.
fn tree_prefixes(visible: &[(String, u16, bool)]) -> Vec<String> {
    let mut last_at: Vec<bool> = Vec::new();
    let mut out = Vec::with_capacity(visible.len());
    for (_, depth, is_last) in visible {
        let d = *depth as usize;
        if last_at.len() <= d {
            last_at.resize(d + 1, false);
        }
        last_at[d] = *is_last;
        let mut p = String::new();
        for is_last_ancestor in &last_at[..d] {
            p.push_str(if *is_last_ancestor { "  " } else { "│ " });
        }
        if d > 0 {
            p.push_str(if *is_last { "└ " } else { "├ " });
        }
        out.push(p);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn draw_body(
    area: Rect,
    tree_w: u16,
    wf_x: u16,
    wf_w: u16,
    buf: &mut Buffer,
    st: &TraceAppState,
    trace: &TraceResponse,
    stats: &TraceStats,
) {
    let visible = st.visible();
    if visible.is_empty() || wf_w == 0 {
        return;
    }
    let prefixes = tree_prefixes(&visible);
    let by_id: std::collections::HashMap<&str, &Span> =
        trace.spans.iter().map(|s| (s.span_id.as_str(), s)).collect();

    let rows = area.height as usize;
    let start = if st.selected < rows { 0 } else { st.selected - rows + 1 };
    let end = (start + rows).min(visible.len());

    let elapsed = st.elapsed_secs();
    let reveal = (elapsed / REVEAL_SECS).min(1.0);
    let pulse = 0.5 + 0.5 * (elapsed * 7.0).sin();
    let dur = trace.duration_us.max(1);
    let pxw = (wf_w as usize) * 2;
    let xof = |off: u64| -> usize {
        (((off as f64) / (dur as f64)) * ((pxw.saturating_sub(1)) as f64)) as usize
    };

    let flow_off = st.flow.as_ref().and_then(|f| f.progress()).map(|p| (p as f64 * dur as f64) as u64);

    let mut cv = Canvas::new(wf_w, area.height);

    for idx in start..end {
        let (id, _depth, _) = &visible[idx];
        let s = match by_id.get(id.as_str()) {
            Some(s) => *s,
            None => continue,
        };
        let r = idx - start;
        let y = area.y + (idx - start) as u16;
        let selected = idx == st.selected;
        let on_crit = stats.critical_set.contains(id);
        let dimmed = st.critical_only && !on_crit;

        // ── left: tree text ──
        let base = span_base_color(s);
        let glyph = if s.is_error() { "◆" } else { "◉" };
        let mut left: Vec<TSpan> = Vec::new();
        if selected {
            left.push(TSpan::styled("❯", Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD)));
        } else {
            left.push(TSpan::raw(" "));
        }
        left.push(TSpan::styled(
            prefixes[idx].clone(),
            Style::default().fg(TEXT_FAINT.to_color()),
        ));
        let glyph_col = if dimmed { TEXT_DIM } else { base };
        left.push(TSpan::styled(format!("{} ", glyph), Style::default().fg(glyph_col.to_color())));
        let name_style = if dimmed {
            Style::default().fg(TEXT_DIM.to_color())
        } else if selected {
            Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_PRIMARY.to_color())
        };
        left.push(TSpan::styled(s.operation.clone(), name_style));
        let dur_w = 8u16;
        let name_w = tree_w.saturating_sub(dur_w);
        Paragraph::new(Line::from(left)).render(Rect::new(area.x, y, name_w, 1), buf);
        // right-aligned duration in tree column
        let dcol = if s.is_error() { ACCENT_ALERT } else { TEXT_SECONDARY };
        Paragraph::new(Line::from(TSpan::styled(fmt_dur_us(s.duration_us), Style::default().fg(dcol.to_color()))))
            .alignment(Alignment::Right)
            .render(Rect::new(area.x + name_w, y, dur_w, 1), buf);

        // ── right: waterfall bar on canvas ──
        let x0 = xof(s.start_offset_us);
        let x1raw = xof(s.end_offset_us());
        let x1 = x0 + (((x1raw.saturating_sub(x0)) as f32) * reveal) as usize;
        // child wait ranges (px)
        let waits: Vec<(usize, usize)> = stats
            .children
            .get(id)
            .map(|kids| {
                kids.iter()
                    .filter_map(|k| by_id.get(k.as_str()))
                    .map(|c| (xof(c.start_offset_us), xof(c.end_offset_us())))
                    .collect()
            })
            .unwrap_or_default();
        let flow_active = flow_off
            .map(|fo| s.start_offset_us <= fo && fo <= s.end_offset_us())
            .unwrap_or(false);

        let ys: &[usize] = if selected { &[0, 1, 2, 3] } else { &[1, 2] };
        let z: i16 = if selected { 10 } else { 5 };
        for x in x0..=x1.min(pxw.saturating_sub(1)) {
            let in_wait = waits.iter().any(|(a, b)| x >= *a && x <= *b);
            let mut col = if in_wait { Canvas::fade(base, 0.30) } else { base };
            if flow_active {
                col = lerp(col, TEXT_PRIMARY, pulse * 0.6);
            }
            if dimmed {
                col = Canvas::fade(col, 0.20);
            }
            for &dy in ys {
                cv.set_px(x, r * 4 + dy, col, z);
            }
        }
        // critical-path glow: bright top edge
        if on_crit && !dimmed {
            let glow = lerp(base, TEXT_PRIMARY, 0.45);
            for x in x0..=x1.min(pxw.saturating_sub(1)) {
                cv.set_px(x, r * 4, glow, z + 1);
            }
        }
        // error span: a hot pulsing cap at the end
        if s.is_error() {
            let hot = lerp(ACCENT_ALERT, TEXT_PRIMARY, pulse * 0.7);
            for dy in 0..4 {
                cv.set_px(x1.min(pxw.saturating_sub(1)), r * 4 + dy, hot, z + 2);
            }
        }
    }

    // flow comet: vertical scan line across the whole body
    if let Some(fo) = flow_off {
        let fx = xof(fo);
        let comet = lerp(ACCENT_OK, TEXT_PRIMARY, pulse);
        for y in 0..cv.px_height() {
            cv.set_px(fx, y, comet, 20);
        }
    }

    cv.blit(buf, Rect::new(wf_x, area.y, wf_w, area.height));
}

fn draw_minimap(
    area: Rect,
    buf: &mut Buffer,
    st: &TraceAppState,
    trace: &TraceResponse,
    stats: &TraceStats,
) {
    if area.width < 4 {
        return;
    }
    let mut cv = Canvas::new(area.width, area.height);
    let pxh = cv.px_height();
    let pxw = cv.px_width();
    let dur = trace.duration_us.max(1);
    let elapsed = st.elapsed_secs();
    let reveal = (elapsed / REVEAL_SECS).min(1.0);
    let xof = |off: u64| -> usize {
        (((off as f64) / (dur as f64)) * ((pxw.saturating_sub(1)) as f64)) as usize
    };

    // each span → a thin line at its depth row
    for s in &trace.spans {
        let depth = (*stats.depth.get(&s.span_id).unwrap_or(&0)).min((pxh as u16).saturating_sub(1));
        let col = span_base_color(s);
        let x0 = xof(s.start_offset_us);
        let x1raw = xof(s.end_offset_us());
        let x1 = x0 + (((x1raw.saturating_sub(x0)) as f32) * reveal) as usize;
        for x in x0..=x1.min(pxw.saturating_sub(1)) {
            cv.set_px(x, depth as usize, col, 5);
        }
    }

    // slow scan line sweeping left→right (faint, low z so bars show through)
    let scan = (elapsed * 0.3).fract();
    let sx = (scan * (pxw.saturating_sub(1)) as f32) as usize;
    for y in 0..pxh {
        cv.set_px(sx, y, Canvas::fade(ACCENT_INFO, 0.45), 3);
    }

    // error markers: full-height bright ticks
    for id in &stats.error_spans {
        if let Some(s) = trace.spans.iter().find(|s| &s.span_id == id) {
            let ex = xof(s.start_offset_us);
            for y in 0..pxh {
                cv.set_px(ex, y, ACCENT_ALERT, 8);
            }
        }
    }

    // selected span window underline on the bottom row
    if let Some(sel) = st.selected_span_id() {
        if let Some(s) = trace.spans.iter().find(|s| s.span_id == sel) {
            let x0 = xof(s.start_offset_us);
            let x1 = xof(s.end_offset_us());
            for x in x0..=x1.min(pxw.saturating_sub(1)) {
                cv.set_px(x, pxh - 1, ACCENT_OK, 9);
            }
        }
    }

    cv.blit(buf, area);
}

fn draw_status_line(
    y: u16,
    x: u16,
    w: u16,
    buf: &mut Buffer,
    trace: &TraceResponse,
    stats: &TraceStats,
) {
    let mut spans: Vec<TSpan> = Vec::new();
    if let Some(eid) = stats.error_spans.first() {
        if let Some(s) = trace.spans.iter().find(|s| &s.span_id == eid) {
            let why = s
                .logs
                .iter()
                .find(|l| l.level == "error")
                .map(|l| l.message.clone())
                .unwrap_or_else(|| "error".into());
            spans.push(TSpan::styled(
                format!(" ◆ {} {} ✗ {}", s.service, s.operation, why),
                Style::default().fg(ACCENT_ALERT.to_color()),
            ));
        }
    } else {
        spans.push(TSpan::styled(" ✓ no errors", Style::default().fg(ACCENT_OK.to_color())));
    }
    if let Some(bid) = &stats.bottleneck {
        if let Some(s) = trace.spans.iter().find(|s| &s.span_id == bid) {
            let self_us = *stats.self_us.get(bid).unwrap_or(&0);
            let pct = self_us as f64 / trace.duration_us.max(1) as f64 * 100.0;
            spans.push(TSpan::styled("   ·   ", Style::default().fg(TEXT_DIM.to_color())));
            spans.push(TSpan::styled(
                format!("bottleneck: {} {} {} ({:.0}%)", s.service, s.operation, fmt_dur_us(self_us), pct),
                Style::default().fg(ACCENT_WARN.to_color()),
            ));
        }
    }
    Paragraph::new(Line::from(spans)).render(Rect::new(x, y, w, 1), buf);
}

// ── summary panel (sidebar + full view share this) ───────────────────────────

fn draw_summary_panel(
    area: Rect,
    buf: &mut Buffer,
    _st: &TraceAppState,
    trace: &TraceResponse,
    stats: &TraceStats,
    full: bool,
) {
    let mut y = area.y;
    let line = |buf: &mut Buffer, y: u16, spans: Vec<TSpan>| {
        if y < area.y + area.height {
            Paragraph::new(Line::from(spans)).render(Rect::new(area.x, y, area.width, 1), buf);
        }
    };

    if !full {
        line(
            buf,
            y,
            vec![TSpan::styled(
                "SUMMARY",
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            )],
        );
        y += 1;
    }

    // wrapped summary sentence
    let sentence = format!("❝ {} ❞", stats.summary);
    let sent_h = if full { 4 } else { 5 };
    Paragraph::new(sentence)
        .style(Style::default().fg(TEXT_PRIMARY.to_color()))
        .wrap(Wrap { trim: true })
        .render(Rect::new(area.x, y, area.width, sent_h), buf);
    y += sent_h + 1;

    // breakdown bars
    line(
        buf,
        y,
        vec![TSpan::styled(
            "SERVICE BREAKDOWN (self-time)",
            Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
        )],
    );
    y += 1;
    let bar_w = area.width.saturating_sub(22) as usize;
    let top = if full { 8 } else { 5 };
    for b in stats.breakdown.iter().take(top) {
        if y >= area.y + area.height - 1 {
            break;
        }
        let col = if b.service == "redis" || stats.error_spans.iter().any(|e| {
            trace.spans.iter().any(|s| &s.span_id == e && s.service == b.service)
        }) {
            ACCENT_ALERT
        } else {
            pod_color(&b.service)
        };
        let bar = bar_str(b.pct, bar_w.max(1));
        line(
            buf,
            y,
            vec![
                TSpan::styled(format!("{:<13}", trunc(&b.service, 13)), Style::default().fg(TEXT_PRIMARY.to_color())),
                TSpan::styled(bar, Style::default().fg(col.to_color())),
                TSpan::styled(
                    format!(" {:>3.0}%", b.pct * 100.0),
                    Style::default().fg(TEXT_SECONDARY.to_color()),
                ),
            ],
        );
        y += 1;
    }
    y += 1;

    // critical path
    if y < area.y + area.height - 1 {
        line(
            buf,
            y,
            vec![TSpan::styled(
                "CRITICAL PATH",
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            )],
        );
        y += 1;
        for id in &stats.critical_path {
            if y >= area.y + area.height {
                break;
            }
            if let Some(s) = trace.spans.iter().find(|s| &s.span_id == id) {
                let is_bottleneck = stats.bottleneck.as_deref() == Some(id.as_str());
                let mut spans = vec![
                    TSpan::styled("◉ ", Style::default().fg(pod_color(&s.service).to_color())),
                    TSpan::styled(
                        format!("{} {}", s.service, s.operation),
                        Style::default().fg(TEXT_PRIMARY.to_color()),
                    ),
                    TSpan::styled(
                        format!("  {} self", fmt_dur_us(*stats.self_us.get(id).unwrap_or(&0))),
                        Style::default().fg(TEXT_DIM.to_color()),
                    ),
                ];
                if is_bottleneck {
                    spans.push(TSpan::styled(
                        "  ← bottleneck",
                        Style::default().fg(ACCENT_WARN.to_color()),
                    ));
                }
                line(buf, y, spans);
                y += 1;
            }
        }
    }
}

fn draw_summary_full(area: Rect, buf: &mut Buffer, st: &TraceAppState) {
    let (trace, stats) = match (st.data.as_ref(), st.stats.as_ref()) {
        (Some(t), Some(s)) => (t, s),
        _ => {
            render_loading(area, buf);
            return;
        }
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(status_border(trace.status).to_color()))
        .title(Line::from(TSpan::styled(
            format!(" Trace Summary · {} ", short_id(&trace.trace_id)),
            Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD),
        )))
        .title_bottom(footer_line(" [s/esc] back to waterfall   [q] quit "));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 8 {
        return;
    }

    // KPI row of 4
    let kpi_h = 4u16;
    let kpi_area = Rect::new(inner.x, inner.y, inner.width, kpi_h);
    let cw = inner.width / 4;
    let errs = stats.error_spans.len();
    let err_svc = stats
        .error_spans
        .first()
        .and_then(|id| trace.spans.iter().find(|s| &s.span_id == id))
        .map(|s| s.service.clone())
        .unwrap_or_else(|| "none".into());
    let depth_max = stats.depth.values().copied().max().unwrap_or(0) + 1;
    let cards = [
        ("TOTAL", fmt_dur_us(trace.duration_us), if errs > 0 { ACCENT_ALERT } else { ACCENT_OK },
         format!("status {}", if errs > 0 { "✗" } else { "✓" })),
        ("SPANS", trace.spans.len().to_string(), ACCENT_INFO, format!("depth {}", depth_max)),
        ("SERVICES", trace.services.len().to_string(), ACCENT_INFO,
         format!("critical {}", stats.critical_path.len())),
        ("ERRORS", errs.to_string(), if errs > 0 { ACCENT_ALERT } else { ACCENT_OK }, err_svc),
    ];
    for (i, (title, value, color, sub)) in cards.into_iter().enumerate() {
        let cx = inner.x + i as u16 * cw;
        KpiCard { title, value, value_color: color, sub }
            .render(Rect::new(cx, kpi_area.y, cw.saturating_sub(1), kpi_h), buf);
    }

    // rest: reuse the shared panel below the KPI row
    let rest = Rect::new(
        inner.x,
        inner.y + kpi_h + 1,
        inner.width,
        inner.height.saturating_sub(kpi_h + 1),
    );
    draw_summary_panel(rest, buf, st, trace, stats, true);
}

// ── span detail ──────────────────────────────────────────────────────────────

fn draw_detail(area: Rect, buf: &mut Buffer, st: &TraceAppState, id: &str) {
    let (trace, stats) = match (st.data.as_ref(), st.stats.as_ref()) {
        (Some(t), Some(s)) => (t, s),
        _ => {
            render_loading(area, buf);
            return;
        }
    };
    let s = match trace.spans.iter().find(|s| s.span_id == id) {
        Some(s) => s,
        None => return,
    };
    let self_us = *stats.self_us.get(id).unwrap_or(&0);
    let status_str = match s.status {
        SpanStatus::Ok => format!("✓ {}", s.status_code.map(|c| c.to_string()).unwrap_or_else(|| "ok".into())),
        SpanStatus::Error => format!("✗ {}", s.status_code.map(|c| c.to_string()).unwrap_or_else(|| "error".into())),
    };
    let head_col = if s.is_error() { ACCENT_ALERT } else { pod_color(&s.service) };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(head_col.to_color()))
        .title(Line::from(TSpan::styled(
            format!(" {} · {} ", s.service, s.operation),
            Style::default().fg(head_col.to_color()).add_modifier(Modifier::BOLD),
        )))
        .title_bottom(footer_line(" [esc] back   [↑↓] sibling spans   [q] quit "));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 6 {
        return;
    }

    let mut y = inner.y;
    let put = |buf: &mut Buffer, y: u16, spans: Vec<TSpan>| {
        Paragraph::new(Line::from(spans)).render(Rect::new(inner.x, y, inner.width, 1), buf);
    };
    let label = |s: &str| TSpan::styled(format!("{:<9}", s), Style::default().fg(TEXT_SECONDARY.to_color()));
    let val = |s: String| TSpan::styled(s, Style::default().fg(TEXT_PRIMARY.to_color()));

    // timing line with a self/wait bar
    let self_frac = self_us as f64 / s.duration_us.max(1) as f64;
    let bar = bar_str(self_frac, 20);
    put(
        buf,
        y,
        vec![
            label("TIMING"),
            val(format!(
                "start +{}   dur {}   self {} ",
                fmt_dur_us(s.start_offset_us),
                fmt_dur_us(s.duration_us),
                fmt_dur_us(self_us)
            )),
            TSpan::styled(bar, Style::default().fg(head_col.to_color())),
            TSpan::styled(format!(" {:.0}% self", self_frac * 100.0), Style::default().fg(TEXT_DIM.to_color())),
        ],
    );
    y += 1;
    put(
        buf,
        y,
        vec![label("KIND"), val(format!("{} {:?}  {}", s.kind.glyph(), s.kind, status_str))],
    );
    y += 2;

    // tags
    put(buf, y, vec![TSpan::styled("TAGS", Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD))]);
    y += 1;
    for (k, v) in &s.tags {
        if y >= inner.y + inner.height - 1 {
            break;
        }
        put(
            buf,
            y,
            vec![
                TSpan::styled(format!("  {:<16}", k), Style::default().fg(ACCENT_INFO.to_color())),
                val(v.clone()),
            ],
        );
        y += 1;
    }
    y += 1;

    // logs
    if !s.logs.is_empty() && y < inner.y + inner.height - 1 {
        put(buf, y, vec![TSpan::styled("LOGS", Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD))]);
        y += 1;
        for l in &s.logs {
            if y >= inner.y + inner.height {
                break;
            }
            let lvl_col = match l.level.as_str() {
                "error" => ACCENT_ALERT,
                "warn" => ACCENT_WARN,
                _ => TEXT_DIM,
            };
            put(
                buf,
                y,
                vec![
                    TSpan::styled(format!("  +{:<8} ", fmt_dur_us(l.at_offset_us)), Style::default().fg(TEXT_DIM.to_color())),
                    TSpan::styled(format!("{:<6}", l.level), Style::default().fg(lvl_col.to_color())),
                    val(l.message.clone()),
                ],
            );
            y += 1;
        }
    }
}

// ── phone ────────────────────────────────────────────────────────────────────

fn draw_phone(area: Rect, buf: &mut Buffer, st: &TraceAppState) {
    let (trace, stats) = match (st.data.as_ref(), st.stats.as_ref()) {
        (Some(t), Some(s)) => (t, s),
        _ => {
            render_loading(area, buf);
            return;
        }
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(status_border(trace.status).to_color()))
        .title(Line::from(TSpan::styled(
            format!(" {} {} ", short_id(&trace.trace_id), fmt_dur_us(trace.duration_us)),
            Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD),
        )))
        .title_bottom(footer_line(" [↑↓] span [⏎] detail [s] sum [q] "));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 3 {
        return;
    }

    let visible = st.visible();
    let by_id: std::collections::HashMap<&str, &Span> =
        trace.spans.iter().map(|s| (s.span_id.as_str(), s)).collect();
    let dur = trace.duration_us.max(1);
    let rows = inner.height as usize;
    let start = if st.selected < rows { 0 } else { st.selected - rows + 1 };

    for (row, idx) in (start..(start + rows).min(visible.len())).enumerate() {
        let (id, depth, _) = &visible[idx];
        let s = match by_id.get(id.as_str()) {
            Some(s) => *s,
            None => continue,
        };
        let y = inner.y + row as u16;
        let selected = idx == st.selected;
        let base = span_base_color(s);
        let glyph = if s.is_error() { "◆" } else { "◉" };
        let indent = "  ".repeat(*depth as usize);
        // mini bar (block eighths) sized to the right portion of the row
        let name_w = (inner.width as usize * 6 / 10).max(8);
        let bar_w = inner.width as usize - name_w - 9;
        let frac = s.duration_us as f64 / dur as f64;
        let bar = bar_str(frac, bar_w.max(1));
        let name = format!("{}{} {}", indent, glyph, trunc(&s.operation, name_w.saturating_sub(indent.len() + 3)));
        let sel = if selected { Modifier::BOLD } else { Modifier::empty() };
        let name_col = if selected { ACCENT_OK } else { TEXT_PRIMARY };
        let mut spans = vec![
            TSpan::styled(
                format!("{:<width$}", trunc(&name, name_w), width = name_w),
                Style::default().fg(name_col.to_color()).add_modifier(sel),
            ),
            TSpan::styled(bar, Style::default().fg(base.to_color())),
        ];
        spans.push(TSpan::styled(
            format!(" {:>7}", fmt_dur_us(s.duration_us)),
            Style::default().fg(TEXT_SECONDARY.to_color()),
        ));
        Paragraph::new(Line::from(spans)).render(Rect::new(inner.x, y, inner.width, 1), buf);
    }
    let _ = stats;
}

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max <= 1 {
        "…".to_string()
    } else {
        let t: String = s.chars().take(max - 1).collect();
        format!("{}…", t)
    }
}
