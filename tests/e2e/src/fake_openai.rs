use axum::{Json, Router, routing::post};
use serde_json::{Value, json};
use tokio::net::TcpListener;

pub struct FakeOpenAi {
    url: String,
    handle: tokio::task::JoinHandle<()>,
}

impl FakeOpenAi {
    pub async fn start() -> Self {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|Json(body): Json<Value>| async move {
                let model = body["model"].as_str().unwrap_or("fake-model").to_string();
                let prompt_text = body["messages"]
                    .as_array()
                    .and_then(|m| m.last())
                    .map(|m| m["content"].to_string())
                    .unwrap_or_default();

                Json(json!({
                    "id": "chatcmpl-test",
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": format!("FakeOpenAI answered: {prompt_text}")
                        }
                    }]
                }))
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self { url: format!("http://{addr}/v1/chat/completions"), handle }
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }
}

impl Drop for FakeOpenAi {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
