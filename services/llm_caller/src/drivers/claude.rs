use crate::drivers::cli::{images_note, run_cli};
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

        let text = run_cli(&self.binary, &args.images, |names| {
            vec!["-p".to_string(), format!("{prompt}{}", images_note(names))]
        })
        .await?;

        Ok(LlmOutput { text, model })
    }
}
