use std::env;
#[derive(Clone, Debug, Default)]
pub struct OpenAIConfig {
    pub openai_key: String,
}
impl OpenAIConfig {
    pub fn init_from_env(&mut self) -> Result<(), String> {
        self.openai_key =
            env::var("OPENAI_KEY").map_err(|_| "OPENAI_KEY not set in environment".to_string())?;

        Ok(())
    }
}
