use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub sensor: String,
    pub llm_caller: String,
    pub system_prompt: String,
    pub text_prompt: String,
    pub user_prompt: String,
}
