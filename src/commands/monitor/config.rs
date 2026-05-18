use figment::providers::{Format, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub monitor: MonitorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(default = "default_range")]
    pub default_range: String,
    #[serde(default = "default_interval")]
    pub default_interval: String,
    #[serde(default)]
    pub endpoint: EndpointConfig,
    #[serde(default)]
    pub aliases: std::collections::HashMap<String, String>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            default_range: default_range(),
            default_interval: default_interval(),
            endpoint: EndpointConfig::default(),
            aliases: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EndpointConfig {
    #[serde(default)]
    pub url: Option<String>,
}

fn default_range() -> String {
    "3h".into()
}
fn default_interval() -> String {
    "60s".into()
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("df")
        .join("config.toml")
}

pub fn load() -> Config {
    let path = config_path();
    let figment = Figment::new().merge(Toml::file(&path));
    figment.extract().unwrap_or_default()
}
