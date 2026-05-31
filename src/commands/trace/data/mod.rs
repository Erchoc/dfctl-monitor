pub mod http;
pub mod mock;
pub mod model;

pub use model::*;

use anyhow::Result;

/// Abstracts where trace data comes from. `MockTraceSource` is the default
/// during development; `HttpTraceSource` will hit the real backend once the
/// endpoint contract is settled (mirrors `monitor::data::DataSource`).
pub trait TraceSource: Send + Sync {
    fn fetch(
        &self,
        trace_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TraceResponse>> + Send + '_>>;
}
