use crate::commands::monitor::render::braille::Canvas;
use crate::commands::monitor::render::colors::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Widget;

pub struct AreaSeries {
    pub points: Vec<f64>,
    pub color: Rgb,
    pub dim: bool,
    pub fill: bool,
}

pub struct AreaChart<'a> {
    pub series: Vec<AreaSeries>,
    pub unit: &'a str,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub y_axis_labels: u8,
    pub show_grid: bool,
    pub cursor_at_end: bool,
    /// Time range to label along the X axis. If set, the chart reserves the last row
    /// for evenly-spaced time labels.
    pub time_range: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
    pub x_label_count: u8,
}

impl<'a> AreaChart<'a> {
    pub fn new(unit: &'a str) -> Self {
        Self {
            series: Vec::new(),
            unit,
            y_min: None,
            y_max: None,
            y_axis_labels: 4,
            show_grid: true,
            cursor_at_end: false,
            time_range: None,
            x_label_count: 7,
        }
    }

    pub fn add(mut self, series: AreaSeries) -> Self {
        self.series.push(series);
        self
    }

    pub fn cursor(mut self, cursor: bool) -> Self {
        self.cursor_at_end = cursor;
        self
    }

    pub fn time_range(
        mut self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        self.time_range = Some((from, to));
        self
    }
}

impl<'a> Widget for AreaChart<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 4 || self.series.is_empty() {
            return;
        }

        // ── Compute Y range once to size the axis label column ──
        let mut tmp_min = f64::INFINITY;
        let mut tmp_max = f64::NEG_INFINITY;
        for s in &self.series {
            for &v in &s.points {
                if v.is_finite() {
                    if v < tmp_min { tmp_min = v; }
                    if v > tmp_max { tmp_max = v; }
                }
            }
        }
        let preview_max = if tmp_max.is_finite() { tmp_max } else { 1.0 };
        let preview_min = if tmp_min.is_finite() { tmp_min.min(0.0) } else { 0.0 };
        // size label column to actual longest formatted value + 1 padding char,
        // clamped to [4, 8] so 24-wide panels still have chart space.
        let widest_label = (0..self.y_axis_labels.max(2) as usize)
            .map(|i| {
                let frac = i as f32 / (self.y_axis_labels.max(2) - 1) as f32;
                let value = preview_max - (preview_max - preview_min) * frac as f64;
                format_y_label(value, self.unit).chars().count()
            })
            .max()
            .unwrap_or(4);
        let label_w = (widest_label as u16 + 1).clamp(4, 8);

        // ── Y axis layout ──
        let chart_x = area.x + label_w;
        let chart_w = area.width.saturating_sub(label_w);
        let chart_y = area.y;
        // Reserve the last row for X-axis labels if we have a time range.
        let x_label_reserved = self.time_range.is_some() && area.height >= 6;
        let chart_h = if x_label_reserved {
            area.height - 1
        } else {
            area.height
        };

        // ── Compute y range ──
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for s in &self.series {
            for &v in &s.points {
                if v.is_finite() {
                    if v < min {
                        min = v;
                    }
                    if v > max {
                        max = v;
                    }
                }
            }
        }
        if !min.is_finite() {
            min = 0.0;
            max = 1.0;
        }
        let raw_max = self.y_max.unwrap_or(max);
        let raw_min = self.y_min.unwrap_or(min.min(0.0));
        let mut y_max = raw_max + (raw_max - raw_min).abs() * 0.10;
        let mut y_min = (raw_min - (raw_max - raw_min).abs() * 0.05).max(if min >= 0.0 { 0.0 } else { f64::NEG_INFINITY });
        // Percentage values can never exceed 100 in the real world; cap the axis so the
        // padding above peak doesn't produce labels like "103%" that erode user trust.
        if self.unit == "%" {
            y_max = y_max.min(100.0);
            y_min = y_min.max(0.0);
        }
        if (y_max - y_min).abs() < 1e-6 {
            y_max = y_min + 1.0;
        }

        // ── Draw Y axis labels ──
        let n_labels = self.y_axis_labels.max(2) as usize;
        for i in 0..n_labels {
            let frac = i as f32 / (n_labels - 1) as f32;
            let value = y_max - (y_max - y_min) * frac as f64;
            let row = chart_y + ((chart_h - 1) as f32 * frac) as u16;
            let label = format_y_label(value, self.unit);
            let span = Span::styled(
                format!("{:>width$}", label, width = (label_w - 1) as usize),
                Style::default().fg(TEXT_SECONDARY.to_color()),
            );
            buf.set_span(area.x, row, &span, label_w - 1);
        }

        // ── Grid dots ──
        if self.show_grid && chart_w > 4 {
            for i in 0..n_labels {
                let frac = i as f32 / (n_labels - 1) as f32;
                let row = chart_y + ((chart_h - 1) as f32 * frac) as u16;
                let mut x = chart_x + 1;
                while x < chart_x + chart_w {
                    if let Some(cell) = buf.cell_mut((x, row)) {
                        if cell.symbol() == " " {
                            cell.set_char('·');
                            cell.set_style(Style::default().fg(TEXT_FAINT.to_color()));
                        }
                    }
                    x += 6;
                }
            }
        }

        // ── Render onto a Braille canvas ──
        let mut canvas = Canvas::new(chart_w, chart_h);
        let px_w = canvas.px_width();
        let px_h = canvas.px_height();
        // In overview panels (≤ 12 cell rows) the glow tail overlaps grid dots and
        // produces speckle. Force fill off there so the chart shows clean edge lines.
        // Single-metric and Phone-detail views have ≥ 18 rows and keep fill enabled.
        let force_no_fill = chart_h < 13;

        // Build per-series average for z-ordering: higher avg → higher z (drawn on top)
        let mut order: Vec<(usize, f64)> = self
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let avg = if s.points.is_empty() {
                    0.0
                } else {
                    s.points.iter().copied().sum::<f64>() / s.points.len() as f64
                };
                (i, avg)
            })
            .collect();
        order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // ── For each series, render line + fade tail ──
        for (z_idx, (i, _avg)) in order.iter().enumerate() {
            let series = &self.series[*i];
            let edge_color = series.color;
            let z = z_idx as i16;
            let n = series.points.len();
            if n < 2 {
                continue;
            }
            // resample to px_w columns
            let mut prev: Option<(i32, i32)> = None;
            for x in 0..px_w {
                let t = x as f64 / (px_w - 1).max(1) as f64;
                let idx_f = t * (n - 1) as f64;
                let i0 = idx_f.floor() as usize;
                let i1 = (i0 + 1).min(n - 1);
                let frac = idx_f - i0 as f64;
                let v = series.points[i0] + (series.points[i1] - series.points[i0]) * frac;
                let v_norm = ((y_max - v) / (y_max - y_min)).clamp(0.0, 1.0);
                let y_px = (v_norm * (px_h - 1) as f64).round() as i32;

                if series.fill && !force_no_fill {
                    // Soft glow underneath the curve with exponential alpha decay so the
                    // result reads as a btop-style filled area without smearing into the
                    // chart floor. Bright series get a deeper tail than dim ones.
                    let glow_depth = if series.dim { 5 } else { 10 };
                    let alpha_peak = if series.dim { 0.32 } else { 0.55 };
                    for dy in 1..=glow_depth {
                        let y = (y_px as usize) + dy;
                        if y >= px_h {
                            break;
                        }
                        let t = 1.0 - (dy as f32 - 1.0) / glow_depth as f32;
                        let alpha = t.powf(1.6) * alpha_peak;
                        if alpha < 0.04 {
                            break;
                        }
                        canvas.set_px(x, y, Canvas::fade(edge_color, alpha), z);
                    }
                }

                // edge line — 2 px thick so flat segments render as full Braille cells,
                // not aliased mosaics.
                if let Some((px, py)) = prev {
                    canvas.line_px(px, py, x as i32, y_px, edge_color, z + 64);
                    canvas.line_px(px, py + 1, x as i32, y_px + 1, edge_color, z + 64);
                }
                canvas.set_px(x, y_px as usize, edge_color, z + 64);
                if (y_px as usize) + 1 < px_h {
                    canvas.set_px(x, (y_px as usize) + 1, edge_color, z + 64);
                }
                prev = Some((x as i32, y_px));
            }
        }

        // ── Cursor at current value ──
        if self.cursor_at_end {
            // dashed-style vertical line at right edge
            let x_px = px_w - 2;
            for y in 0..px_h {
                if y % 3 == 0 {
                    canvas.set_px(x_px, y, TEXT_DIM, i16::MAX - 1);
                }
            }
            // marker dot at each series' current value
            for (z_idx, (i, _avg)) in order.iter().enumerate() {
                let series = &self.series[*i];
                if let Some(&v) = series.points.last() {
                    let v_norm = ((y_max - v) / (y_max - y_min)).clamp(0.0, 1.0);
                    let y_px = (v_norm * (px_h - 1) as f64).round() as usize;
                    let z = z_idx as i16 + 200;
                    canvas.set_px(x_px, y_px, series.color, z);
                    canvas.set_px(x_px + 1, y_px, series.color, z);
                    if y_px > 0 {
                        canvas.set_px(x_px, y_px - 1, series.color, z);
                    }
                    if y_px + 1 < px_h {
                        canvas.set_px(x_px, y_px + 1, series.color, z);
                    }
                }
            }
        }

        canvas.blit(buf, Rect::new(chart_x, chart_y, chart_w, chart_h));

        // ── X axis time labels ──
        if let Some((from, to)) = self.time_range {
            if x_label_reserved && chart_w > 20 {
                let n = self.x_label_count.max(2) as usize;
                let label_row = chart_y + chart_h;
                let span_secs = (to - from).num_seconds().max(60) as f64;
                let use_full = span_secs >= 86_400.0;
                for i in 0..n {
                    let frac = i as f64 / (n - 1) as f64;
                    let t = from + chrono::Duration::seconds((span_secs * frac) as i64);
                    let local = t.with_timezone(&chrono::Local);
                    let label = if use_full {
                        local.format("%m-%d %H:%M").to_string()
                    } else {
                        local.format("%H:%M").to_string()
                    };
                    let label_chars = label.chars().count();
                    let center = (chart_w as f64 * frac) as u16;
                    let mut x = chart_x + center;
                    // shift left so the label center matches the tick position
                    let shift = (label_chars / 2) as u16;
                    if x >= chart_x + shift {
                        x -= shift;
                    }
                    if x + label_chars as u16 > chart_x + chart_w {
                        x = chart_x + chart_w - label_chars as u16;
                    }
                    let span = Span::styled(
                        label,
                        Style::default().fg(TEXT_SECONDARY.to_color()),
                    );
                    buf.set_span(x, label_row, &span, label_chars as u16);
                }
            }
        }
    }
}

pub fn format_y_label(value: f64, unit: &str) -> String {
    match unit {
        "%" => {
            if value.abs() < 10.0 {
                format!("{:.1}%", value)
            } else {
                format!("{:.0}%", value)
            }
        }
        "ms" => format!("{:.0}ms", value),
        "GB" => format!("{:.1}G", value),
        "bytes" => format_bytes(value),
        "rpm" => format_traffic(value),
        "/s" | "qps" => format_traffic(value * 60.0),
        _ => format_decimal(value),
    }
}

pub fn format_traffic(rpm: f64) -> String {
    let qps = rpm / 60.0;
    if qps < 1.0 {
        format!("{:.0}/m", rpm)
    } else if qps < 1000.0 {
        format!("{:.0}/s", qps)
    } else {
        format!("{:.1}K/s", qps / 1000.0)
    }
}

fn format_bytes(bytes: f64) -> String {
    let kb = bytes / 1024.0;
    if kb < 1024.0 {
        format!("{:.0}K", kb)
    } else if kb < 1024.0 * 1024.0 {
        format!("{:.1}M", kb / 1024.0)
    } else {
        format!("{:.1}G", kb / 1024.0 / 1024.0)
    }
}

fn format_decimal(value: f64) -> String {
    if value.abs() >= 10_000.0 {
        format!("{:.1}k", value / 1000.0)
    } else if value.abs() >= 100.0 {
        format!("{:.0}", value)
    } else if value.abs() >= 10.0 {
        format!("{:.1}", value)
    } else {
        format!("{:.2}", value)
    }
}

