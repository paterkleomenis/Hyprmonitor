use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub name: String,
    #[serde(default)]
    pub active: bool,
    #[serde(skip)]
    pub modes: BTreeMap<String, Vec<f64>>,
}

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub resolution: String,
    pub refresh_rate: f64,
    pub scale: i32,
    pub resolution_index: usize,
    pub refresh_rate_index: usize,
    pub dpms_on: bool,
}

impl MonitorConfig {
    pub fn scale_as_float(&self) -> f64 {
        self.scale as f64 / 100.0
    }
}
