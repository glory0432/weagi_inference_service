use std::sync::Arc;

use crate::controllers::image;
use crate::ServiceState;
use axum::routing::post;

pub fn add_routers(router: axum::Router<Arc<ServiceState>>) -> axum::Router<Arc<ServiceState>> {
    router.route("/api/chat/image", post(image::image_generate))
}
