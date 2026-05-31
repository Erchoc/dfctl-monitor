//! KPI card widget — moved to shared `crate::tui::kpi_card`. Re-exported so
//! monitor's `widgets::kpi_card::KpiCard` call-sites keep working.
pub use crate::tui::kpi_card::KpiCard;
