pub mod chat;
use std::sync::Arc;

use crate::ServiceState;
use axum::Router;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
pub fn create_router(state: Arc<ServiceState>) -> Router {
    let router = Router::new();
    let router = chat::add_routers(router);
    router.with_state(state).layer(
        TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default().include_headers(true)),
    )
}
