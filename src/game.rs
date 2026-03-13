use rand::seq::SliceRandom;
use std::{collections::HashSet, sync::Arc};
use tapaculo::*;
use tokio::sync::RwLock;

use crate::messages::{GameEvent, GameMessage};

// ── Board logic ──────────────────────────────────────────────────────────────

/// Check a flat 9-cell slice (row-major 3×3) for a winner.
/// Mirrors the Svelte calculateWinner — iterates rows and columns in a loop,
/// then checks the two diagonals.
fn check_winner(cells: &[Option<char>]) -> Option<char> {
  for i in 0..3 {
    // row i
    let r = cells[i * 3];
    if r.is_some() && r == cells[i * 3 + 1] && r == cells[i * 3 + 2] {
      return r;
    }
    // column i
    let c = cells[i];
    if c.is_some() && c == cells[i + 3] && c == cells[i + 6] {
      return c;
    }
  }
  let d1 = cells[0];
  if d1.is_some() && d1 == cells[4] && d1 == cells[8] {
    return d1;
  }
  let d2 = cells[2];
  if d2.is_some() && d2 == cells[4] && d2 == cells[6] {
    return d2;
  }
  None
}

pub fn sub_board_done(cells: &[Option<char>]) -> bool {
  check_winner(cells).is_some() || cells.iter().all(|c| c.is_some())
}

pub fn board_is_full(board: &[Vec<Option<char>>]) -> bool {
  board.iter().all(|sub| sub.iter().all(|c| c.is_some()))
}

// ── Game state ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Snapshot {
  pub board: Vec<Vec<Option<char>>>,
  pub squares_winner: Vec<Option<char>>,
  pub next_board: Option<u8>,
  pub current_turn: String,
}

/// Authoritative game state stored in the room via tapaculo's custom state API.
#[derive(Clone)]
pub struct GameState {
  /// 9 sub-boards × 9 cells — null | 'x' | 'o', matching the Svelte client
  pub board: Vec<Vec<Option<char>>>,
  /// Winner of each sub-board (null | 'x' | 'o')
  pub squares_winner: Vec<Option<char>>,
  pub naughts: String,
  pub crosses: String,
  pub current_turn: String,
  /// Which sub-board the current player must play in (None = any available)
  pub next_board: Option<u8>,
  pub history: Vec<Snapshot>,
}

impl GameState {
  pub fn new(naughts: String, crosses: String) -> Self {
    let current_turn = naughts.clone(); // 'o' (naughts) goes first
    Self {
      board: vec![vec![None; 9]; 9],
      squares_winner: vec![None; 9],
      naughts,
      crosses,
      current_turn,
      next_board: None,
      history: Vec::new(),
    }
  }

  /// Returns Some('x') for crosses, Some('o') for naughts, None if not a player.
  pub fn player_mark(&self, user_id: &str) -> Option<char> {
    if user_id == self.crosses {
      Some('x')
    } else if user_id == self.naughts {
      Some('o')
    } else {
      None
    }
  }

  pub fn board_history(&self) -> Vec<Vec<Vec<Option<char>>>> {
    self.history.iter().map(|s| s.board.clone()).collect()
  }
}

// ── Room event handler ────────────────────────────────────────────────────────

pub struct GameEventHandler {
  pub rooms: Arc<RwLock<HashSet<String>>>,
}

#[async_trait::async_trait]
impl RoomEventHandler for GameEventHandler {
  async fn on_player_joined(&self, ctx: &Context, user_id: &str) {
    tracing::info!("Player {} joined room {}", user_id, ctx.room_id());
    let _ = ctx
      .broadcast_to_others(GameEvent::PlayerJoined {
        user_id: user_id.to_string(),
      })
      .await;
    if let Some(info) = ctx.get_room_info().await {
      if info.member_count == 1 {
        let _ = ctx.send_to(user_id, GameEvent::WaitingForOpponent).await;
      }
    }
  }

  async fn on_room_full(&self, ctx: &Context) {
    // If game state already exists, this is a reconnection — send current state to rejoining player
    if let Some(state) = ctx.get_custom_state::<GameState>().await {
      tracing::info!(
        "Player {} reconnected to room {}",
        ctx.user_id(),
        ctx.room_id()
      );
      let _ = ctx
        .send_to(
          ctx.user_id(),
          GameEvent::GameRejoined {
            naughts: state.naughts.clone(),
            crosses: state.crosses.clone(),
            updated_board: state.board.clone(),
            squares_winner: state.squares_winner.clone(),
            history: state.board_history(),
            next_board: state.next_board,
          },
        )
        .await;
      return;
    }

    let mut members = ctx.get_room_members().await;
    if members.len() == 2 {
      members.shuffle(&mut rand::thread_rng());
      let naughts = members[0].clone();
      let crosses = members[1].clone();
      tracing::info!(
        "Starting game in room {}: {} (o) vs {} (x)",
        ctx.room_id(),
        naughts,
        crosses
      );
      let state = GameState::new(naughts.clone(), crosses.clone());
      let _ = ctx.set_custom_state(state).await;
      let _ = ctx
        .broadcast(GameEvent::GameStart { naughts, crosses })
        .await;
    }
  }

  async fn on_player_left(&self, ctx: &Context, user_id: &str) {
    tracing::info!("Player {} left room {}", user_id, ctx.room_id());
    let _ = ctx
      .broadcast_to_others(GameEvent::PlayerLeft {
        user_id: user_id.to_string(),
      })
      .await;
  }

  async fn on_room_empty(&self, room_id: &str) {
    tracing::info!("Room {} is now empty, removing code", room_id);
    self.rooms.write().await.remove(room_id);
  }
}

// ── Message handler ───────────────────────────────────────────────────────────

pub async fn handle_game_message(ctx: Context, envelope: Envelope<GameMessage>) {
  tracing::debug!("Message from {}: {:?}", envelope.from, envelope.data);

  match envelope.data {
    GameMessage::Move {
      board: new_board,
      square,
    } => {
      let Some(state) = ctx.get_custom_state::<GameState>().await else {
        return;
      };

      if state.current_turn != envelope.from {
        let _ = ctx
          .send_to(
            &envelope.from,
            GameEvent::InvalidMove {
              reason: "Not your turn".to_string(),
            },
          )
          .await;
        return;
      }

      let Some(mark) = state.player_mark(&envelope.from) else {
        return;
      };

      if new_board.len() != 9 || new_board.iter().any(|sub| sub.len() != 9) {
        let _ = ctx
          .send_to(
            &envelope.from,
            GameEvent::InvalidMove {
              reason: "Invalid board dimensions".to_string(),
            },
          )
          .await;
        return;
      }

      // Find what changed between stored board and submitted board
      let mut changed: Vec<(usize, usize)> = Vec::new();
      let mut invalid_reason: Option<String> = None;

      'outer: for (bi, (old_sub, new_sub)) in
        state.board.iter().zip(new_board.iter()).enumerate()
      {
        for (ci, (&old_cell, &new_cell)) in old_sub.iter().zip(new_sub.iter()).enumerate() {
          if old_cell != new_cell {
            if old_cell.is_some() {
              invalid_reason = Some("Cell already occupied".to_string());
              break 'outer;
            }
            if new_cell != Some(mark) {
              invalid_reason = Some("Wrong mark for this player".to_string());
              break 'outer;
            }
            changed.push((bi, ci));
          }
        }
      }

      if let Some(reason) = invalid_reason {
        let _ = ctx
          .send_to(&envelope.from, GameEvent::InvalidMove { reason })
          .await;
        return;
      }

      if changed.len() != 1 {
        let _ = ctx
          .send_to(
            &envelope.from,
            GameEvent::InvalidMove {
              reason: format!("Expected exactly 1 change, got {}", changed.len()),
            },
          )
          .await;
        return;
      }

      let (changed_board_idx, changed_square_idx) = changed[0];

      if let Some(required) = state.next_board {
        if changed_board_idx as u8 != required {
          let _ = ctx
            .send_to(
              &envelope.from,
              GameEvent::InvalidMove {
                reason: format!("Must play in sub-board {}", required),
              },
            )
            .await;
          return;
        }
      }

      if changed_square_idx as u8 != square {
        let _ = ctx
          .send_to(
            &envelope.from,
            GameEvent::InvalidMove {
              reason: "Claimed square does not match the cell that changed".to_string(),
            },
          )
          .await;
        return;
      }

      // Move is valid — snapshot, apply, update derived state
      let snapshot = Snapshot {
        board: state.board.clone(),
        squares_winner: state.squares_winner.clone(),
        next_board: state.next_board,
        current_turn: state.current_turn.clone(),
      };

      let mut new_squares_winner = state.squares_winner.clone();
      if new_squares_winner[changed_board_idx].is_none() {
        new_squares_winner[changed_board_idx] = check_winner(&new_board[changed_board_idx]);
      }

      let next_board = if sub_board_done(&new_board[square as usize]) {
        None
      } else {
        Some(square)
      };

      let next_turn = if envelope.from == state.crosses {
        state.naughts.clone()
      } else {
        state.crosses.clone()
      };

      let naughts = state.naughts.clone();
      let crosses = state.crosses.clone();

      let mut new_state = state;
      new_state.history.push(snapshot);
      new_state.board = new_board.clone();
      new_state.squares_winner = new_squares_winner.clone();
      new_state.next_board = next_board;
      new_state.current_turn = next_turn;

      let board_history = new_state.board_history();
      let _ = ctx.set_custom_state(new_state).await;

      let _ = ctx
        .broadcast(GameEvent::MoveMade {
          by: envelope.from.clone(),
          square,
          updated_board: new_board.clone(),
          squares_winner: new_squares_winner.clone(),
          history: board_history,
          next_board,
        })
        .await;

      if let Some(winner_mark) = check_winner(&new_squares_winner) {
        let winner = if winner_mark == 'x' { crosses } else { naughts };
        let _ = ctx
          .broadcast(GameEvent::GameOver {
            winner: Some(winner.clone()),
            reason: format!("{} wins", winner),
          })
          .await;
      } else if board_is_full(&new_board) {
        let _ = ctx
          .broadcast(GameEvent::GameOver {
            winner: None,
            reason: "Draw — board is full".to_string(),
          })
          .await;
      }
    }

    GameMessage::Resign => {
      let members = ctx.get_room_members().await;
      let winner = members.into_iter().find(|id| id != &envelope.from);
      let _ = ctx
        .broadcast(GameEvent::GameOver {
          winner,
          reason: format!("{} resigned", envelope.from),
        })
        .await;
    }

    GameMessage::OfferDraw => {
      let _ = ctx
        .broadcast_to_others(GameEvent::DrawOffered {
          by: envelope.from.clone(),
        })
        .await;
    }

    GameMessage::AcceptDraw => {
      let _ = ctx
        .broadcast(GameEvent::GameOver {
          winner: None,
          reason: "Draw by agreement".to_string(),
        })
        .await;
    }

    GameMessage::RequestUndo => {
      let _ = ctx
        .broadcast_to_others(GameEvent::UndoRequested {
          by: envelope.from.clone(),
        })
        .await;
    }

    GameMessage::AcceptUndo => {
      let Some(mut state) = ctx.get_custom_state::<GameState>().await else {
        return;
      };

      let Some(snapshot) = state.history.pop() else {
        let _ = ctx
          .send_to(
            &envelope.from,
            GameEvent::InvalidMove {
              reason: "No moves to undo".to_string(),
            },
          )
          .await;
        return;
      };

      state.board = snapshot.board;
      state.squares_winner = snapshot.squares_winner;
      state.next_board = snapshot.next_board;
      state.current_turn = snapshot.current_turn;

      let updated_board = state.board.clone();
      let squares_winner = state.squares_winner.clone();
      let board_history = state.board_history();
      let next_board = state.next_board;

      let _ = ctx.set_custom_state(state).await;

      let _ = ctx
        .broadcast(GameEvent::UndoAccepted {
          updated_board,
          squares_winner,
          history: board_history,
          next_board,
        })
        .await;
    }
  }
}
