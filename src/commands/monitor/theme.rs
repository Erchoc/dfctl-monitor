use super::data::{MetricKind, Series, SeriesKind};
use super::render::colors::*;

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

pub fn series_color(metric: MetricKind, series: &Series) -> Rgb {
    match &series.kind {
        SeriesKind::Percentile(p) => match p {
            50 => SERIES_GREEN,
            95 => SERIES_ORANGE,
            99 => SERIES_PINK,
            _ => SERIES_BLUE,
        },
        SeriesKind::StatusCode(c) => status_color(*c),
        SeriesKind::Pod(name) => pod_color(name),
        SeriesKind::Component(c) => match c.as_str() {
            "hsf" | "rpc" => SERIES_BLUE,
            "db" | "mysql" | "postgres" => SERIES_ORANGE,
            "redis" | "cache" => SERIES_PINK,
            _ => SERIES_GREEN,
        },
        SeriesKind::Single => match metric {
            MetricKind::ErrorRate => ACCENT_ALERT,
            MetricKind::Cpu => SERIES_ORANGE,
            MetricKind::Memory => SERIES_BLUE,
            MetricKind::Runtime => SERIES_PINK,
            _ => SERIES_GREEN,
        },
    }
}


pub fn assess_health(metric: MetricKind, series: &[Series]) -> HealthStatus {
    match metric {
        MetricKind::ErrorRate => {
            if let Some(s) = series.first() {
                let v = s.current();
                if v > 5.0 {
                    return HealthStatus::Alert;
                }
                if v > 2.0 {
                    return HealthStatus::Warn;
                }
            }
            HealthStatus::Ok
        }
        MetricKind::Latency => {
            if let Some(p99) = series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(99))) {
                let stats = p99.stats();
                if stats.max > 250.0 {
                    return HealthStatus::Alert;
                }
                if stats.max > 150.0 {
                    return HealthStatus::Warn;
                }
            }
            HealthStatus::Ok
        }
        MetricKind::Cpu => {
            let max_now = series.iter().find(|s| s.label == "max").map(|s| s.current()).unwrap_or(0.0);
            if max_now > 85.0 {
                return HealthStatus::Alert;
            }
            if max_now > 70.0 {
                return HealthStatus::Warn;
            }
            HealthStatus::Ok
        }
        MetricKind::Memory => HealthStatus::Ok,
        MetricKind::Qps => {
            let s5xx = series.iter().find(|s| matches!(s.kind, SeriesKind::StatusCode(c) if c >= 500));
            if let Some(s) = s5xx {
                if s.current() > 100.0 {
                    return HealthStatus::Warn;
                }
            }
            HealthStatus::Ok
        }
        MetricKind::Upstream => {
            let max_now = series.iter().map(|s| s.current()).fold(0.0_f64, f64::max);
            if max_now > 100.0 {
                return HealthStatus::Warn;
            }
            HealthStatus::Ok
        }
        _ => HealthStatus::Ok,
    }
}
