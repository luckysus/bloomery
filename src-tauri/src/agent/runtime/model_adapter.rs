use crate::providers::capabilities::{
    ChatEvent, ChatProvider, ChatRequest, ChatResponse, ProviderCapabilities,
};
use crate::providers::http::ProviderError;
use std::future::Future;
use std::pin::Pin;

pub type ModelFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ChatResponse, ProviderError>> + Send + 'a>>;

pub trait ModelAdapter: Send + Sync {
    fn capabilities(&self) -> &ProviderCapabilities;

    fn generate<'a>(
        &'a self,
        request: ChatRequest,
        on_event: &'a mut (dyn FnMut(ChatEvent) + Send),
        is_cancelled: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ModelFuture<'a>;
}

pub struct ProviderModelAdapter<P> {
    provider: P,
}

impl<P> ProviderModelAdapter<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }
}

impl<P> ModelAdapter for ProviderModelAdapter<P>
where
    P: ChatProvider,
{
    fn capabilities(&self) -> &ProviderCapabilities {
        self.provider.capabilities()
    }

    fn generate<'a>(
        &'a self,
        request: ChatRequest,
        on_event: &'a mut (dyn FnMut(ChatEvent) + Send),
        is_cancelled: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ModelFuture<'a> {
        Box::pin(async move { self.provider.chat(request, on_event, is_cancelled).await })
    }
}
