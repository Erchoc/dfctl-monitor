pub mod mock;
pub mod model;

pub use model::*;

use anyhow::Result;

pub trait DataSource: Send + Sync {
    fn fetch(
        &self,
        query: MonitorQuery,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MonitorResponse>> + Send + '_>>;
}
