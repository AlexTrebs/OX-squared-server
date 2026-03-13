use std::{env, time::Duration};

pub struct Config {
  pub listen_addr: String,
  pub cors_origin: String,
  pub jwt_secret: String,
  pub token_ttl_secs: usize,
  pub room_timeout: Duration,
  pub max_history_messages: usize,
  pub rate_limit_max_messages: u32,
  pub rate_limit_window: Duration,
  pub rate_limit_ban: Duration,
  pub max_message_size_bytes: usize,
}

impl Config {
  pub fn from_env() -> Self {
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
      tracing::warn!("JWT_SECRET not set — using insecure default. Set it before deploying.");
      "change-me".to_string()
    });

    Self {
      listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
      cors_origin: env::var("CORS_ORIGIN").unwrap_or_else(|_| "*".to_string()),
      jwt_secret,
      token_ttl_secs: parse_env("TOKEN_TTL_SECS", 3600),
      room_timeout: Duration::from_secs(parse_env("ROOM_TIMEOUT_SECS", 300)),
      max_history_messages: parse_env("MAX_HISTORY_MESSAGES", 100),
      rate_limit_max_messages: parse_env("RATE_LIMIT_MAX_MESSAGES", 20),
      rate_limit_window: Duration::from_secs(parse_env("RATE_LIMIT_WINDOW_SECS", 1)),
      rate_limit_ban: Duration::from_secs(parse_env("RATE_LIMIT_BAN_SECS", 60)),
      max_message_size_bytes: parse_env("MAX_MESSAGE_SIZE_BYTES", 64 * 1024),
    }
  }
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
  env::var(key)
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(default)
}
