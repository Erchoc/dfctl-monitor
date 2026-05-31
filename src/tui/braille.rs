//! Braille pixel canvas — each terminal cell is a 2×4 pixel block.

use crate::tui::colors::{lerp, Rgb, BG};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

// Braille dot bitmap mapping: (x, y) → bit
// dots in a 2×4 grid:
//   (0,0) (1,0)
//   (0,1) (1,1)
//   (0,2) (1,2)
//   (0,3) (1,3)
const DOT_BITS: [[u32; 4]; 2] = [
    [0x01, 0x02, 0x04, 0x40],
    [0x08, 0x10, 0x20, 0x80],
];

#[derive(Clone, Copy)]
struct Cell {
    bits: u32,
    color: Rgb,
    z: i16,
}

pub struct Canvas {
    cols: u16, // in cells
    rows: u16, // in cells
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            cells: vec![
                Cell {
                    bits: 0,
                    color: BG,
                    z: i16::MIN,
                };
                (cols as usize) * (rows as usize)
            ],
        }
    }

    pub fn px_width(&self) -> usize {
        (self.cols as usize) * 2
    }
    pub fn px_height(&self) -> usize {
        (self.rows as usize) * 4
    }

    /// Set a pixel (in px coords) with z-ordering: only overwrites if z ≥ existing.
    pub fn set_px(&mut self, x: usize, y: usize, color: Rgb, z: i16) {
        if x >= self.px_width() || y >= self.px_height() {
            return;
        }
        let cx = x / 2;
        let cy = y / 4;
        let idx = cy * (self.cols as usize) + cx;
        let bit = DOT_BITS[x % 2][y % 4];
        let cell = &mut self.cells[idx];
        if z >= cell.z {
            cell.color = color;
            cell.bits |= bit;
            cell.z = z;
        } else if cell.bits == 0 {
            // first dot in this cell: take the color anyway
            cell.color = color;
            cell.bits |= bit;
            cell.z = z;
        } else {
            // keep existing color but still mark the bit (so the curve is continuous)
            cell.bits |= bit;
        }
    }

    /// Draw a line in pixel coordinates between two points (Bresenham).
    #[allow(dead_code)] // used by trace connectors / future diagonal callers
    pub fn line_px(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Rgb,
        z: i16,
    ) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;
        loop {
            if x >= 0 && y >= 0 {
                self.set_px(x as usize, y as usize, color, z);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Compute a faded color from background, used for fade-tail fills.
    pub fn fade(base: Rgb, amount: f32) -> Rgb {
        lerp(BG, base, amount.clamp(0.0, 1.0))
    }

    /// Blit the canvas into the buffer at position (x, y).
    pub fn blit(&self, buf: &mut Buffer, area: Rect) {
        let max_cols = (area.width as usize).min(self.cols as usize);
        let max_rows = (area.height as usize).min(self.rows as usize);
        for r in 0..max_rows {
            for c in 0..max_cols {
                let cell = self.cells[r * (self.cols as usize) + c];
                if cell.bits == 0 {
                    continue;
                }
                let ch = char::from_u32(0x2800 + cell.bits).unwrap_or(' ');
                if let Some(out) = buf.cell_mut((area.x + c as u16, area.y + r as u16)) {
                    out.set_char(ch);
                    out.set_style(Style::default().fg(cell.color.to_color()));
                }
            }
        }
    }
}
