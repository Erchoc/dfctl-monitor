use crate::tui::colors::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthStatus {
    Ok,
    Warn,
    Alert,
}

impl HealthStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            HealthStatus::Ok => "OK",
            HealthStatus::Warn => "WARN",
            HealthStatus::Alert => "ALERT",
        }
    }

    pub fn color(&self) -> Rgb {
        match self {
            HealthStatus::Ok => ACCENT_OK,
            HealthStatus::Warn => ACCENT_WARN,
            HealthStatus::Alert => ACCENT_ALERT,
        }
    }

    /// Border tint for panels (chrome). Kept subtle so the eye lands on data,
    /// not on a wall of colored frames. The badge/title carry the actual status
    /// signal in saturated colour.
    pub fn border_color(&self) -> Rgb {
        match self {
            // OK and WARN both use the same dim grey — the warning is communicated
            // by the title badge, the chart spike colour, and the ▲ glyph, not by
            // the frame around the panel.
            HealthStatus::Ok | HealthStatus::Warn => BORDER_DIM,
            // ALERT keeps a tinted frame (saturated red) because something is on fire.
            HealthStatus::Alert => ACCENT_ALERT,
        }
    }
}
