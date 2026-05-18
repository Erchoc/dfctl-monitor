use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b)
    }

    pub fn to_color(self) -> Color {
        Color::Rgb(self.0, self.1, self.2)
    }

    pub fn from_hex(hex: u32) -> Self {
        Self(
            ((hex >> 16) & 0xff) as u8,
            ((hex >> 8) & 0xff) as u8,
            (hex & 0xff) as u8,
        )
    }
}

pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

// ── palette ──────────────────────────────────────────────────────────────────

pub const BG: Rgb = Rgb::new(0x0d, 0x11, 0x17);
pub const BORDER_DIM: Rgb = Rgb::new(0x3a, 0x3a, 0x3a);
pub const BORDER_GRID: Rgb = Rgb::new(0x25, 0x25, 0x25);
pub const TEXT_PRIMARY: Rgb = Rgb::new(0xff, 0xff, 0xff);
pub const TEXT_SECONDARY: Rgb = Rgb::new(0x8a, 0x8a, 0x8a);
pub const TEXT_DIM: Rgb = Rgb::new(0x5a, 0x5a, 0x5a);
pub const TEXT_FAINT: Rgb = Rgb::new(0x3a, 0x3a, 0x3a);

pub const ACCENT_OK: Rgb = Rgb::new(0x00, 0xd4, 0xaa);
pub const ACCENT_WARN: Rgb = Rgb::new(0xff, 0xb8, 0x6c);
pub const ACCENT_ALERT: Rgb = Rgb::new(0xff, 0x6b, 0x6b);
pub const ACCENT_INFO: Rgb = Rgb::new(0x7c, 0x9e, 0xff);
pub const ACCENT_SECONDARY: Rgb = Rgb::new(0xff, 0x79, 0xc6);

// extra named palette tokens used in charts
pub const SERIES_BLUE: Rgb = Rgb::new(0x7c, 0x9e, 0xff);
pub const SERIES_GREEN: Rgb = Rgb::new(0x50, 0xe0, 0xa0);
pub const SERIES_ORANGE: Rgb = Rgb::new(0xff, 0xb8, 0x6c);
pub const SERIES_PINK: Rgb = Rgb::new(0xff, 0x79, 0xc6);
pub const SERIES_RED: Rgb = Rgb::new(0xff, 0x6b, 0x6b);
pub const SERIES_YELLOW: Rgb = Rgb::new(0xf1, 0xfa, 0x8c);

pub fn pod_color(name: &str) -> Rgb {
    let mut h: u32 = 5381;
    for b in name.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    match h % 5 {
        0 => SERIES_BLUE,
        1 => SERIES_GREEN,
        2 => SERIES_ORANGE,
        3 => SERIES_PINK,
        _ => SERIES_YELLOW,
    }
}

pub fn status_color(code: u16) -> Rgb {
    match code {
        c if (200..300).contains(&c) => ACCENT_OK,
        c if (300..400).contains(&c) => ACCENT_INFO,
        c if (400..500).contains(&c) => ACCENT_WARN,
        _ => ACCENT_ALERT,
    }
}
