use axum::{extract::State, http::StatusCode, Json};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};
use tapaculo::JwtAuth;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct HttpState {
  pub rooms: Arc<RwLock<HashSet<String>>>,
  pub jwt_secret: String,
  pub token_ttl_secs: usize,
}

#[derive(Deserialize)]
pub struct CreateGameRequest {
  pub user_id: String,
}

#[derive(Deserialize)]
pub struct JoinGameRequest {
  pub user_id: String,
  pub code: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
  pub token: String,
}

#[derive(Serialize)]
pub struct CreateGameResponse {
  pub code: String,
  pub token: String,
}

pub async fn create_game(
  State(state): State<HttpState>,
  Json(body): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, StatusCode> {
  let mut rooms = state.rooms.write().await;
  let code = generate_code(&rooms);
  rooms.insert(code.clone());

  let token = sign_token(&state.jwt_secret, &body.user_id, &code, state.token_ttl_secs)
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

  Ok(Json(CreateGameResponse { code, token }))
}

pub async fn join_game(
  State(state): State<HttpState>,
  Json(body): Json<JoinGameRequest>,
) -> Result<Json<TokenResponse>, StatusCode> {
  let rooms = state.rooms.read().await;
  if !rooms.contains(&body.code) {
    return Err(StatusCode::NOT_FOUND);
  }

  let token = sign_token(&state.jwt_secret, &body.user_id, &body.code, state.token_ttl_secs)
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

  Ok(Json(TokenResponse { token }))
}

fn generate_code(existing: &HashSet<String>) -> String {
  const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  let mut rng = rand::thread_rng();
  loop {
    let code: String = (0..5)
      .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
      .collect();
    if !existing.contains(&code) {
      return code;
    }
  }
}

fn sign_token(secret: &str, user_id: &str, room_id: &str, ttl_secs: usize) -> anyhow::Result<String> {
  let mut rng = rand::thread_rng();
  let session_id = format!("{:016x}", rng.r#gen::<u64>());
  JwtAuth::new(secret).sign_access(
    user_id.to_string(),
    room_id.to_string(),
    session_id,
    ttl_secs,
  )
}
