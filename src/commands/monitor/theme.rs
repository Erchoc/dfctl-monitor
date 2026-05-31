use super::data::{MetricKind, Series, SeriesKind};
use super::render::colors::*;

// HealthStatus now lives in the shared TUI layer (reused by `trace`). Re-exported
// so `theme::HealthStatus` references throughout monitor keep resolving.
pub use crate::tui::health::HealthStatus;

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


/// Evaluate panel health.
///
/// API-supplied `thresholds` (from `MetricData::thresholds`) always win. If
/// none are provided, fall back to built-in defaults so the front-end still
/// shows *some* badge — but those defaults are conservative and the platform
/// team should always send real thresholds with the data.
pub fn assess_health_with_thresholds(
    metric: MetricKind,
    series: &[Series],
    thresholds: Option<&crate::commands::monitor::data::Thresholds>,
) -> HealthStatus {
    if let Some(t) = thresholds {
        let probe = t
            .watch_series
            .as_deref()
            .and_then(|name| series.iter().find(|s| s.label == name))
            // Per-metric defaults when API didn't specify watch_series:
            //  - Latency → P95 (matches CURRENT KPI / panel headline)
            //  - CPU/Memory → "max" aggregate if present
            //  - Anything else → first non-pod series
            .or_else(|| match metric {
                MetricKind::Latency => series
                    .iter()
                    .find(|s| matches!(s.kind, crate::commands::monitor::data::SeriesKind::Percentile(95)))
                    .or_else(|| {
                        series.iter().find(|s| {
                            matches!(s.kind, crate::commands::monitor::data::SeriesKind::Percentile(99))
                        })
                    }),
                MetricKind::Cpu | MetricKind::Memory => series.iter().find(|s| s.label == "max"),
                _ => None,
            })
            .or_else(|| series.iter().find(|s| !matches!(s.kind, crate::commands::monitor::data::SeriesKind::Pod(_))))
            .or_else(|| series.first());
        if let Some(s) = probe {
            let v = s.current();
            if let Some(at) = t.alert_above {
                if v > at {
                    return HealthStatus::Alert;
                }
            }
            if let Some(wt) = t.warn_above {
                if v > wt {
                    return HealthStatus::Warn;
                }
            }
            return HealthStatus::Ok;
        }
    }
    assess_health(metric, series)
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
            // Fallback thresholds when API doesn't supply any. Watch P95 (not
            // P99) for the same reason CURRENT and preview do — P99 is the
            // tail and over-triggers ALERT on bursty workloads.
            if let Some(p95) = series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(95))) {
                let v = p95.current();
                if v > 200.0 {
                    return HealthStatus::Alert;
                }
                if v > 120.0 {
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
