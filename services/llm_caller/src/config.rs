use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    #[default]
    Agy,
    Claude,
    Http,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub kind: DriverKind,
    pub binary: String,
    pub model: String,
    pub endpoint: String,
    pub api_key: String,
}
