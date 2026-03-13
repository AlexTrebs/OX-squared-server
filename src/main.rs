mod config;
mod game;
mod http;
mod messages;

use axum::{http::HeaderValue, routing::post};
use std::{collections::HashSet, sync::Arc};
use tapaculo::*;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

use config::Config;
use game::{GameEventHandler, handle_game_message};
use http::{HttpState, create_game, join_game};
use messages::GameMessage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  dotenv::dotenv().ok();

  tracing_subscriber::fmt()
    .with_env_filter("ox-squared-server=debug,tapaculo=info")
    .init();

  let cfg = Config::from_env();

  let cors = build_cors(&cfg.cors_origin);

  // Shared room registry — both the HTTP handlers and the event handler hold a reference
  let rooms = Arc::new(RwLock::new(HashSet::new()));

  let http_state = HttpState {
    rooms: rooms.clone(),
    jwt_secret: cfg.jwt_secret.clone(),
    token_ttl_secs: cfg.token_ttl_secs,
  };

  let http_router = Router::new()
    .route("/create-game", post(create_game))
    .route("/join-game", post(join_game))
    .with_state(http_state);

  let app = Server::new()
    .with_auth(JwtAuth::new(&cfg.jwt_secret))
    .with_pubsub(InMemoryPubSub::new())
    .with_room_settings(RoomSettings {
      max_players: Some(2),
      allow_spectators: false,
      store_message_history: true,
      max_history_messages: cfg.max_history_messages,
      empty_room_timeout: Some(cfg.room_timeout),
    })
    .with_limits(MessageLimits {
      max_size_bytes: cfg.max_message_size_bytes,
      max_messages_per_window: cfg.rate_limit_max_messages,
      window_duration: cfg.rate_limit_window,
      ban_duration: cfg.rate_limit_ban,
    })
    .with_token_ttl(cfg.token_ttl_secs)
    .with_token_endpoint()
    .with_event_handler(GameEventHandler { rooms })
    .on_message_typed::<GameMessage, _, _>(handle_game_message)
    .on_message_validate(|_ctx, envelope| {
      if envelope.from.is_empty() {
        return Err("Invalid sender".to_string());
      }
      Ok(())
    })
    .into_router()
    .merge(http_router)
    .layer(cors);

  let listener = tokio::net::TcpListener::bind(&cfg.listen_addr).await?;
  tracing::info!("Listening on {}", cfg.listen_addr);
  axum::serve(listener, app).await?;

  Ok(())
}

fn build_cors(origin: &str) -> CorsLayer {
  if origin == "*" {
    CorsLayer::new()
      .allow_origin(Any)
      .allow_methods(Any)
      .allow_headers(Any)
  } else {
    let origins: Vec<HeaderValue> = origin
      .split(',')
      .filter_map(|o| o.trim().parse().ok())
      .collect();
    CorsLayer::new()
      .allow_origin(origins)
      .allow_methods(Any)
      .allow_headers(Any)
  }
}
