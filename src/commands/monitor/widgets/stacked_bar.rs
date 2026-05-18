use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::widgets::chart::format_y_label;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Widget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficDisplay {
    Auto,  // pick by median
    Rpm,
    Qps,
}

pub struct StackedBar<'a> {
    pub series: Vec<StackedSeries>,
    pub unit: &'a str,
    pub display: TrafficDisplay,
}

pub struct StackedSeries {
    pub label: String,
    pub color: Rgb,
    pub points: Vec<f64>,
}

const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

impl<'a> Widget for StackedBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 4 || self.series.is_empty() {
            return;
        }
        let label_w: u16 = 6;
        let chart_x = area.x + label_w;
        let chart_w = area.width.saturating_sub(label_w);
        let chart_h = area.height;

        let n_points = self.series[0].points.len();
        if n_points == 0 {
            return;
        }

        // totals per column
        let mut totals = vec![0.0_f64; n_points];
        for s in &self.series {
            for (i, &v) in s.points.iter().enumerate() {
                if i < totals.len() {
                    totals[i] += v.max(0.0);
                }
            }
        }
        let mut max_total = totals.iter().copied().fold(0.0_f64, f64::max);
        if max_total <= 0.0 {
            max_total = 1.0;
        }
        max_total *= 1.10;

        // Pick whether to display as RPM or QPS based on data median (avoids horizontal
        // unit jumps), unless the caller forced a specific unit.
        let display_unit = if self.unit == "rpm" {
            let median = traffic_median(&self.series);
            match self.display {
                TrafficDisplay::Rpm => "rpm",
                TrafficDisplay::Qps => "qps",
                TrafficDisplay::Auto => {
                    if median / 60.0 >= 1.0 {
                        "qps"
                    } else {
                        "rpm"
                    }
                }
            }
        } else {
            self.unit
        };

        // Y-axis labels
        let n_labels = 4_usize;
        for i in 0..n_labels {
            let frac = i as f32 / (n_labels - 1) as f32;
            let value = max_total - (max_total) * frac as f64;
            let row = area.y + ((chart_h - 1) as f32 * frac) as u16;
            let label = match display_unit {
                "qps" => format!("{:.0}/s", value / 60.0),
                "rpm" => format!("{:.0}/m", value),
                _ => format_y_label(value, display_unit),
            };
            let span = Span::styled(
                format!("{:>width$}", label, width = (label_w - 1) as usize),
                Style::default().fg(TEXT_SECONDARY.to_color()),
            );
            buf.set_span(area.x, row, &span, label_w - 1);
        }

        // Each column is 1 cell wide; we paint every-other column to leave a 1-cell
        // gap that visually separates adjacent bars (btop-style spacing).
        let total_subunits = (chart_h as f64) * 8.0;
        let scale = total_subunits / max_total;
        let bar_stride: usize = 2;
        let bar_count = (chart_w as usize + bar_stride - 1) / bar_stride;

        for bar_idx in 0..bar_count {
            let col = bar_idx * bar_stride;
            if col >= chart_w as usize {
                break;
            }
            let t = if bar_count > 1 {
                bar_idx as f64 / (bar_count - 1) as f64
            } else {
                0.0
            };
            let idx_f = t * (n_points - 1) as f64;
            let i0 = idx_f.floor() as usize;
            let i1 = (i0 + 1).min(n_points - 1);
            let frac = idx_f - i0 as f64;

            // accumulator from bottom upward in subunits
            let mut cursor_subunits = 0.0_f64;
            for s in &self.series {
                let v = (s.points[i0] + (s.points[i1] - s.points[i0]) * frac).max(0.0);
                let seg = v * scale;
                let start = cursor_subunits;
                let end = cursor_subunits + seg;
                paint_segment(buf, chart_x + col as u16, area.y, chart_h, start, end, s.color);
                cursor_subunits = end;
            }
        }
    }
}

fn paint_segment(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    h: u16,
    start: f64,
    end: f64,
    color: Rgb,
) {
    // each cell row from bottom up represents 8 subunits.
    // row index from bottom: row = floor(s/8), within-cell = s%8
    let h = h as i32;
    let s0 = start.max(0.0);
    let s1 = end;
    if s1 <= s0 {
        return;
    }
    for r in 0..h {
        // r=0 is bottom
        let cell_bottom = (r as f64) * 8.0;
        let cell_top = cell_bottom + 8.0;
        let overlap_lo = s0.max(cell_bottom);
        let overlap_hi = s1.min(cell_top);
        if overlap_hi <= overlap_lo {
            if cell_bottom >= s1 {
                break;
            }
            continue;
        }
        let lo_in_cell = overlap_lo - cell_bottom; // 0..8
        let hi_in_cell = overlap_hi - cell_bottom;
        let row_y = (y + h as u16 - 1) - r as u16;
        let cell = buf.cell_mut((x, row_y));
        if cell.is_none() {
            continue;
        }
        let cell = cell.unwrap();
        let cur_ch = cell.symbol().chars().next().unwrap_or(' ');
        let cur_filled = block_to_height(cur_ch);
        // we render only the topmost stack piece visually: if our segment ends above any
        // existing filled portion, replace; else if it starts above existing top, append by
        // extending the block character upward.
        // For simplicity: if our hi_in_cell is higher than current filled height, overwrite.
        let new_height = hi_in_cell.round().clamp(0.0, 8.0) as usize;
        if new_height >= cur_filled || cur_ch == ' ' {
            cell.set_char(BLOCKS[new_height]);
            cell.set_style(Style::default().fg(color.to_color()));
        }
        let _ = lo_in_cell;
    }
}

fn block_to_height(ch: char) -> usize {
    BLOCKS.iter().position(|&c| c == ch).unwrap_or(0)
}

fn traffic_median(series: &[StackedSeries]) -> f64 {
    // Sum across status codes per column → take median of sums.
    if series.is_empty() {
        return 0.0;
    }
    let n_points = series[0].points.len();
    if n_points == 0 {
        return 0.0;
    }
    let mut totals: Vec<f64> = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let sum: f64 = series.iter().map(|s| s.points.get(i).copied().unwrap_or(0.0)).sum();
        totals.push(sum);
    }
    totals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    totals[totals.len() / 2]
}
