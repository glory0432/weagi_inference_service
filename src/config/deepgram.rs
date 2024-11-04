use std::env;
#[derive(Clone, Debug, Default)]
pub struct DeepgramConfig {
    pub deepgram_key: String,
}
impl DeepgramConfig {
    pub fn init_from_env(&mut self) -> Result<(), String> {
        self.deepgram_key = env::var("DEEPGRAM_KEY")
            .map_err(|_| "DEEPGRAM_KEY not set in environment".to_string())?;

        Ok(())
    }
}
