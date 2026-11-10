use crate::drivers::{Driver, ImageArg, PromptArgs};
use async_trait::async_trait;
use contracts::{ILlmCallerService, LlmCallRequest, LlmCallResponse};
use sdk::state::IStateStore;
use sdk::{ProgressSender, RpcError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmInstanceState {
    pub total_calls: u64,
    pub last_model: String,
}

pub struct LlmCallerHandler {
    driver: Arc<dyn Driver>,
    state: Arc<dyn IStateStore<LlmInstanceState>>,
}

impl LlmCallerHandler {
    pub fn new(
        driver: Arc<dyn Driver>,
        state: Arc<dyn IStateStore<LlmInstanceState>>,
    ) -> Self {
        Self { driver, state }
    }
}

#[async_trait]
impl ILlmCallerService for LlmCallerHandler {
    #[tracing::instrument(skip(self, req, progress))]
    async fn call(
        &self,
        req: LlmCallRequest,
        progress: ProgressSender,
    ) -> Result<LlmCallResponse, RpcError> {
        if req.user_prompt.trim().is_empty() {
            return Err(RpcError::invalid_arg("user_prompt must not be empty"));
        }

        progress.report(format!("invoking LLM ({} images)", req.images.len()));

        let args = PromptArgs {
            system_prompt: req.system_prompt,
            text_prompt: req.text_prompt,
            user_prompt: req.user_prompt,
            images: req
                .images
                .into_iter()
                .map(|i| ImageArg {
                    data: i.data,
                    mime_type: i.mime_type,
                })
                .collect(),
        };

        let out = self.driver.call(args).await?;

        let model_used = out.model.clone();
        self.state
            .update_cas(Box::new(move |s| {
                s.total_calls += 1;
                s.last_model = model_used.clone();
            }))
            .await
            .map_err(|e| RpcError::internal(format!("llm state CAS failed: {e}")))?;

        progress.report("response received");
        tracing::info!(model = %out.model, chars = out.text.len(), "LLM call succeeded");

        Ok(LlmCallResponse {
            text: out.text,
            model: out.model,
        })
    }
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod handler_tests;
