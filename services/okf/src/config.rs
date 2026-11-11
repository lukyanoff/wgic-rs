use serde::{Deserialize, Serialize};

/// `--config '{"root":"./okf-data"}'`. The writer and its readers must point at the same folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub root: String,
}

impl Default for Config {
    fn default() -> Self {
        Self { root: "./okf-data".to_string() }
    }
}
