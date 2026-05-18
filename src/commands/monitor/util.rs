use chrono::{DateTime, Duration as ChronoDuration, Utc};

pub fn format_duration_short(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3600;
    let mins = (seconds % 3600) / 60;
    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

pub fn ago(at: DateTime<Utc>) -> String {
    let now = Utc::now();
    let d: ChronoDuration = now - at;
    let secs = d.num_seconds().max(0) as u64;
    format_duration_short(secs)
}
