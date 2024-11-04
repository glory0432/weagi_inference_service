use crate::ServiceState;
use std::sync::Arc;
use tower_http::services::ServeDir;

pub fn add_routers(router: axum::Router<Arc<ServiceState>>) -> axum::Router<Arc<ServiceState>> {
    router
        .nest_service("/api/chat/public/images", ServeDir::new("./public/images"))
        .nest_service("/api/chat/public/voice", ServeDir::new("./public/voice"))
}
