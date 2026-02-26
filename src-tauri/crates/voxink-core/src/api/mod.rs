use std::time::Duration;

/// HTTP connect timeout (matching Android OkHttp config)
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// HTTP read/write timeout (matching Android OkHttp config)
pub const READ_WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// Groq API base URL
pub const GROQ_BASE_URL: &str = "https://api.groq.com/";
/// OpenAI API base URL
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/";
/// OpenRouter API base URL
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/";

pub mod groq;
