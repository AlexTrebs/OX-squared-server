use serde::{Deserialize, Serialize};

/// Messages sent from client to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GameMessage {
  Move {
    /// Full board after the player's move: 9 sub-boards × 9 cells (null | "x" | "o")
    board: Vec<Vec<Option<char>>>,
    /// The square index (0–8) played, determines opponent's next required sub-board
    square: u8,
  },
  Resign,
  OfferDraw,
  AcceptDraw,
  RequestUndo,
  AcceptUndo,
}

/// Events broadcast from server to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GameEvent {
  GameStart {
    naughts: String,
    crosses: String,
  },
  MoveMade {
    by: String,
    square: u8,
    updated_board: Vec<Vec<Option<char>>>,
    squares_winner: Vec<Option<char>>,
    history: Vec<Vec<Vec<Option<char>>>>,
    /// Which sub-board the next player must play in (None = any available)
    next_board: Option<u8>,
  },
  GameOver {
    winner: Option<String>,
    reason: String,
  },
  DrawOffered {
    by: String,
  },
  UndoRequested {
    by: String,
  },
  /// Sent after an undo is accepted — same board-state fields as MoveMade
  /// so the client can handle both with the same applyServerMove function.
  UndoAccepted {
    updated_board: Vec<Vec<Option<char>>>,
    squares_winner: Vec<Option<char>>,
    history: Vec<Vec<Vec<Option<char>>>>,
    next_board: Option<u8>,
  },
  PlayerJoined {
    user_id: String,
  },
  PlayerLeft {
    user_id: String,
  },
  WaitingForOpponent,
  InvalidMove {
    reason: String,
  },
}
