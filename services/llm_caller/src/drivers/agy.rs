use crate::drivers::cli::{images_note, run_cli};
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

        let text = run_cli(&self.binary, &args.images, |names| {
            // agy is an agent that may run commands unasked: keep it sandboxed and in plan mode.
            let flags = ["--sandbox", "--mode", "plan", "--disable-slash-commands", "-p"];
            let mut cli_args: Vec<String> = flags.into_iter().map(String::from).collect();
            cli_args.push(format!("{prompt}{}", images_note(names)));
            cli_args
        })
        .await?;

        Ok(LlmOutput { text, model })
    }
}
