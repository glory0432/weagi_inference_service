use std::env;
use std::net::{AddrParseError, SocketAddr};

#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
    pub addr: String,
    pub port: u16,
}

impl ServerConfig {
    pub fn get_addr(&self) -> String {
        format!("{}:{}", self.addr, self.port)
    }

    pub fn get_http_addr(&self) -> String {
        format!("http://{}:{}", self.addr, self.port)
    }

    pub fn get_socket_addr(&self) -> Result<SocketAddr, AddrParseError> {
        self.get_addr().parse()
    }

    pub fn init_from_env(&mut self) -> Result<(), String> {
        self.addr = env::var("SERVER_ADDR")
            .map_err(|_| "SERVER_ADDR not set in environment".to_string())?;

        self.port = env::var("SERVER_PORT")
            .map_err(|_| "SERVER_PORT not set in environment".to_string())?
            .parse::<u16>()
            .map_err(|_| "SERVER_PORT is not a valid u16".to_string())?;

        Ok(())
    }
}
