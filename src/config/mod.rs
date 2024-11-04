pub mod constant;
pub mod db;
pub mod deepgram;
pub mod jwt;
pub mod openai;
pub mod server;
pub mod tracing;

use dotenv::dotenv;

#[derive(Clone, Default, Debug)]
pub struct ServiceConfig {
    pub db: db::DatabaseConfig,
    pub server: server::ServerConfig,
    pub jwt: jwt::JWTConfig,
    pub openai: openai::OpenAIConfig,
    pub deepgram: deepgram::DeepgramConfig,
}

impl ServiceConfig {
    pub fn init_from_env(&mut self) -> Result<(), String> {
        dotenv().ok();
        self.db.init_from_env()?;
        self.server.init_from_env()?;
        self.jwt.init_from_env()?;
        self.openai.init_from_env()?;
        self.deepgram.init_from_env()?;
        Ok(())
    }
}
