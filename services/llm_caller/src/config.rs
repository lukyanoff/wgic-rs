//! Settings, read once at startup. One running instance = one model setup; a request never changes them.

use kernel::Deps;

/// One instance's settings (≈ a C# options class).
#[derive(Debug, Clone, Default)]
pub struct LlmSettings {
    /// Instance id, e.g. `llm-claude` (in results and logs). Default `llm`.
    pub id: String,
    /// `claude-cli` | `agy-cli` | `openai` | `openrouter`. Required.
    pub driver: String,
    /// Required, e.g. `claude-sonnet-5-5`.
    pub model: String,
    /// Passed through as is; the CLI or provider rejects values it doesn't know. Default `medium`.
    pub effort: String,
    /// HTTP drivers: replaces the provider's `https://.../v1` (tests, proxies).
    pub base_url: Option<String>,
    /// CLI drivers: replaces the `claude` / `agy` program (tests).
    pub cli_path: Option<String>,
}

impl LlmSettings {
    /// `LLM_CALLER_DRIVER`, `_MODEL` (both required), `_ID`, `_EFFORT`, `_BASE_URL`, `_CLI_PATH`.
    pub fn from_env(deps: &Deps) -> Self {
        Self {
            id: deps.env("LLM_CALLER_ID").unwrap_or_else(|| "llm".into()),
            driver: deps.required("LLM_CALLER_DRIVER", "claude-cli, agy-cli, openai or openrouter"),
            model: deps.required("LLM_CALLER_MODEL", "a model name, e.g. claude-sonnet-5-5"),
            effort: deps.env("LLM_CALLER_EFFORT").unwrap_or_else(|| "medium".into()),
            base_url: deps.env("LLM_CALLER_BASE_URL").map(|url| url.trim_end_matches('/').to_string()),
            cli_path: deps.env("LLM_CALLER_CLI_PATH"),
        }
    }
}
