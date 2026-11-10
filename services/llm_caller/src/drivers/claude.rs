use crate::drivers::cli::run_cli;
use crate::drivers::{Driver, LlmOutput, PromptArgs};
use async_trait::async_trait;
use sdk::RpcError;

pub struct ClaudeDriver {
    binary: String,
    model: String,
}

impl ClaudeDriver {
    pub fn new(binary: String, model: String) -> Self {
        Self { binary, model }
    }
}

#[async_trait]
impl Driver for ClaudeDriver {
    async fn call(&self, args: PromptArgs) -> Result<LlmOutput, RpcError> {
        let prompt = args.combined_prompt();
        let model = self.model.clone();

        let text = run_cli(&self.binary, &args.images, |image_paths| {
            let mut full_prompt = prompt;
            for path in image_paths {
                full_prompt.push_str(&format!("\n[Image: {}]", path.display()));
            }
            vec!["-p".to_string(), full_prompt]
        })
        .await?;

        Ok(LlmOutput { text, model })
    }
}
