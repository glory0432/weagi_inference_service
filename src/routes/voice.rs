use std::sync::Arc;

use crate::controllers::voice;
use crate::ServiceState;
use axum::routing::post;

pub fn add_routers(router: axum::Router<Arc<ServiceState>>) -> axum::Router<Arc<ServiceState>> {
    router.route("/api/chat/voice", post(voice::speech_to_text))
}
