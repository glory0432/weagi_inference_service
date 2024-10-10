mod client;
mod config;
mod controllers;
mod dto;
mod entity;
mod repositories;
mod routes;
mod service;
mod utils;

use crate::{
    client::db::{DatabaseClient, DatabaseClientExt},
    config::{tracing::subscribe_tracing, ServiceConfig},
    routes::create_router,
};
use std::sync::Arc;
use tracing::{error, info};

#[derive(Clone)]
pub struct ServiceState {
    pub config: Arc<ServiceConfig>,
    pub db: Arc<DatabaseClient>,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    subscribe_tracing();

    let mut service_config = config::ServiceConfig::default();
    service_config.init_from_env().map_err(|e| {
        error!("💥 Error in loading configuration from env: {}", e);
        e
    })?;
    info!("✔ Configuration data is loaded!");

    let db_client = DatabaseClient::build_from_config(&service_config)
        .await
        .map_err(|e| {
            error!("💥 Error in database connection: {}", e);
            "Failed to build database client" // Provide a descriptive error message
        })?;
    info!("✔ Connected to the database!");

    let service_state = Arc::new(ServiceState {
        config: Arc::new(service_config.clone()),
        db: Arc::new(db_client),
    });

    let listener_addr = service_config
        .clone()
        .server
        .get_socket_addr()
        .map_err(|e| {
            error!("💥 Failed to get socket address: {}", e);
            "Invalid socket address"
        })?;
    let tcp_listener = tokio::net::TcpListener::bind(listener_addr)
        .await
        .map_err(|e| {
            error!("💥 Failed to bind TCP listener: {}", e);
            "Failed to bind TCP listener"
        })?;

    let addr = tcp_listener.local_addr().map_err(|e| {
        error!("💥 Failed to get addr of the listener: {}", e);
        "Failed to get local listener address"
    })?;
    info!("🚀 The server is listening on: {}", addr); // Move logging before serving

    let router = create_router(service_state);
    axum::serve(tcp_listener, router).await.map_err(|e| {
        error!("💥 Server error: {}", e);
        "Server error occurred"
    })?;

    Ok(())
}
