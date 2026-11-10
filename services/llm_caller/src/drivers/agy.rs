use crate::drivers::cli::run_cli;
use crate::drivers::{Driver, LlmOutput, PromptArgs};
use async_trait::async_trait;
use sdk::RpcError;

pub struct AgyDriver {
    binary: String,
    model: String,
}

impl AgyDriver {
    pub fn new(binary: String, model: String) -> Self {
        Self { binary, model }
    }
}

#[async_trait]
impl Driver for AgyDriver {
    async fn call(&self, args: PromptArgs) -> Result<LlmOutput, RpcError> {
        let prompt = args.combined_prompt();
        let model = self.model.clone();

        let text = run_cli(&self.binary, &args.images, |image_paths| {
            let mut cli_args = vec!["--prompt".to_string(), prompt];
            for path in image_paths {
                cli_args.push("--image".to_string());
                cli_args.push(path.to_string_lossy().into_owned());
            }
            cli_args
        })
        .await?;

        Ok(LlmOutput { text, model })
    }
}
