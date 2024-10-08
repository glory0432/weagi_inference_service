use std::env;
#[derive(Clone, Debug, Default)]
pub struct JWTConfig {
    pub refresh_token_expired_date: u64,
    pub access_token_expired_date: u64,
    pub refresh_token_secret: String,
    pub access_token_secret: String,
}
impl JWTConfig {
    pub fn init_from_env(&mut self) -> Result<(), String> {
        self.refresh_token_expired_date = env::var("JWT_REFRESH_TOKEN_EXPIRED_DATE")
            .map_err(|_| "JWT_REFRESH_TOKEN_EXPIRED_DATE not set in environment".to_string())?
            .parse::<u64>()
            .map_err(|_| "JWT_REFRESH_TOKEN_EXPIRED_DATE is not a valid u64".to_string())?;

        self.access_token_expired_date = env::var("JWT_ACCESS_TOKEN_EXPIRED_DATE")
            .map_err(|_| "JWT_ACCESS_TOKEN_EXPIRED_DATE not set in environment".to_string())?
            .parse::<u64>()
            .map_err(|_| "JWT_ACCESS_TOKEN_EXPIRED_DATE is not a valid u64".to_string())?;

        self.refresh_token_secret = env::var("JWT_REFRESH_TOKEN_SECRET")
            .map_err(|_| "JWT_REFRESH_TOKEN_SECRET not set in environment".to_string())?;

        self.access_token_secret = env::var("JWT_ACCESS_TOKEN_SECRET")
            .map_err(|_| "JWT_ACCESS_TOKEN_SECRET not set in environment".to_string())?;

        Ok(())
    }
}
