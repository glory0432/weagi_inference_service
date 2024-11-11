pub mod chat;
pub mod image;
pub mod public;
pub mod voice;
use std::sync::Arc;

use crate::ServiceState;
use axum::{extract::DefaultBodyLimit, Router};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
pub fn create_router(state: Arc<ServiceState>) -> Router {
    let router = Router::new();
    let router = chat::add_routers(router);
    let router = public::add_routers(router);
    let router = voice::add_routers(router);
    let router = image::add_routers(router);
    let router = router.layer(DefaultBodyLimit::max(300 * 1024 * 1024));
    router.with_state(state).layer(
        TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default().include_headers(true)),
    )
}
