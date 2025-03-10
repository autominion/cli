use std::sync::Arc;

use actix_web::{web, Error, HttpRequest, Scope};
use once_cell::sync::Lazy;
use serde_json::Value;
use url::Url;

use llm_proxy::{CompletionRequest, ProxyConfig};

use crate::context::Context;

static OPENROUTER_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://openrouter.ai/api/v1/chat/completions")
        .expect("Failed to parse OpenRouter chat completions URL")
});

pub fn scope() -> Scope {
    llm_proxy::scope(TheProxyConfig {})
}

#[derive(Clone)]
struct TheProxyConfig {}

impl ProxyConfig for TheProxyConfig {
    type Context = Arc<Context>;

    async fn extract_context(&self, req: &HttpRequest) -> Result<Self::Context, Error> {
        let ctx = req
            .app_data::<web::Data<Context>>()
            .expect("Context not found in app data");
        let ctx = ctx.clone().into_inner();

        Ok(ctx)
    }

    async fn api_key(
        &self,
        ctx: &Self::Context,
        _req: &CompletionRequest,
    ) -> Result<String, Error> {
        Ok(ctx.openrouter_key.clone())
    }

    async fn forward_to_url(
        &self,
        _ctx: &Self::Context,
        _req: &CompletionRequest,
    ) -> Result<Url, Error> {
        Ok(OPENROUTER_CHAT_COMPLETIONS_URL.clone())
    }

    async fn inspect_interaction(
        &self,
        _ctx: &Self::Context,
        request: &CompletionRequest,
        response: Option<Value>,
    ) {
        // For now we just log raw request and response
        // Later we will need to come up with a proper feedback mechanism
        println!("Request: {:?}\n\nResponse: {:?}", request, response);
    }
}
