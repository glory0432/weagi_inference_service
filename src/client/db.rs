use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::config::ServiceConfig;

pub type DatabaseClient = DatabaseConnection;

pub trait DatabaseClientExt: Sized {
    fn build_from_config(
        config: &ServiceConfig,
    ) -> impl std::future::Future<Output = Result<DatabaseConnection, String>>;
}

impl DatabaseClientExt for DatabaseClient {
    async fn build_from_config(config: &ServiceConfig) -> Result<DatabaseConnection, String> {
        let mut opt = ConnectOptions::new(config.db.get_url());
        opt.max_connections(100)
            .min_connections(5)
            .connect_timeout(Duration::from_secs(8))
            .acquire_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8))
            .max_lifetime(Duration::from_secs(8));
        let db = Database::connect(opt)
            .await
            .map_err(|e| format!("Error in connectiong to database: {}", e))?;
        Ok(db)
    }
}
