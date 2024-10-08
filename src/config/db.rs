use std::env;
#[derive(Debug, Clone, Default)]
pub struct DatabaseConfig {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database: String,
}

impl DatabaseConfig {
    pub fn get_url(&self) -> String {
        Self::create_url(
            &self.username,
            &self.password,
            &self.host,
            self.port,
            &self.database,
        )
    }

    pub fn create_url(
        username: &str,
        password: &str,
        host: &str,
        port: u16,
        database_name: &str,
    ) -> String {
        format!("postgres://{username}:{password}@{host}:{port}/{database_name}")
    }

    pub fn init_from_env(&mut self) -> Result<(), String> {
        self.username = env::var("DB_USERNAME")
            .map_err(|_| "DB_USERNAME not set in environment".to_string())?;
        self.password = env::var("DB_PASSWORD")
            .map_err(|_| "DB_PASSWORD not set in environment".to_string())?;
        self.host =
            env::var("DB_HOST").map_err(|_| "DB_HOST not set in environment".to_string())?;

        self.port = env::var("DB_PORT")
            .map_err(|_| "DB_PORT not set in environment".to_string())?
            .parse::<u16>()
            .map_err(|_| "DB_PORT is not a valid u16".to_string())?;

        self.database = env::var("DB_DATABASE")
            .map_err(|_| "DB_DATABASE not set in environment".to_string())?;

        Ok(())
    }
}
