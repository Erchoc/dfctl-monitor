use crate::commands::monitor::render::colors::Rgb;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn sparkline(buf: &mut Buffer, area: Rect, values: &[f64], color: Rgb) {
    if values.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let w = area.width as usize;
    let n = values.len();
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in values {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    if !min.is_finite() {
        return;
    }
    if (max - min).abs() < 1e-9 {
        max = min + 1.0;
    }
    for col in 0..w {
        let t = if w > 1 {
            col as f64 / (w - 1) as f64
        } else {
            0.0
        };
        let idx_f = t * (n - 1) as f64;
        let i0 = idx_f.floor() as usize;
        let i1 = (i0 + 1).min(n - 1);
        let frac = idx_f - i0 as f64;
        let v = values[i0] + (values[i1] - values[i0]) * frac;
        let norm = ((v - min) / (max - min)).clamp(0.0, 1.0);
        let bucket = (norm * 7.999) as usize;
        if let Some(cell) = buf.cell_mut((area.x + col as u16, area.y)) {
            cell.set_char(BLOCKS[bucket]);
            cell.set_style(Style::default().fg(color.to_color()));
        }
    }
}
