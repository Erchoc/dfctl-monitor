//! HTTP-backed trace source.
//!
//! Stubbed for now — mirrors `monitor::data` which also defers the real HTTP
//! client until the backend contract is finalized. Wiring this up is the
//! "real data" milestone (T8): add `reqwest`, build the request from the
//! configured endpoint + trace id, and deserialize into `TraceResponse`.

use super::{TraceResponse, TraceSource};
use anyhow::{anyhow, Result};
use std::future::Future;
use std::pin::Pin;

#[allow(dead_code)]
pub struct HttpTraceSource {
    pub endpoint: String,
}

impl TraceSource for HttpTraceSource {
    fn fetch(
        &self,
        _trace_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TraceResponse>> + Send + '_>> {
        Box::pin(async move {
            Err(anyhow!(
                "HttpTraceSource not implemented yet — run without an endpoint to use mock data"
            ))
        })
    }
}
