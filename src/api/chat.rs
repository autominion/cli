use std::sync::Arc;

use actix_web::{web, Error, HttpRequest, Scope};
use serde_json::Value;
use url::Url;

use llm_proxy::{CompletionRequest, ProxyConfig};

use crate::context::Context;

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
        Ok(ctx.llm_provider_details.api_key.clone())
    }

    async fn forward_to_url(
        &self,
        ctx: &Self::Context,
        _req: &CompletionRequest,
    ) -> Result<Url, Error> {
        Ok(ctx
            .llm_provider_details
            .api_chat_completions_endpoint
            .clone())
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
