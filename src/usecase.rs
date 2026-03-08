use crate::model::{BOARD_SIZE, Cell, GameState, GameStatus, Pos};

#[derive(Clone, Debug)]
struct MoveRecord {
    player: Cell,
    pos: Pos,
    flipped: usize,
    note: Option<String>,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    state: GameState,
    last_move: Option<Pos>,
    last_pass: Option<Cell>,
    move_records: Vec<MoveRecord>,
}

#[derive(Clone, Debug)]
pub struct GameViewModel {
    pub board: [[Cell; BOARD_SIZE]; BOARD_SIZE],
    pub legal_moves: Vec<Pos>,
    pub turn: Cell,
    pub black_score: usize,
    pub white_score: usize,
    pub message: String,
    pub game_over: bool,
    pub last_move: Option<Pos>,
    pub history_lines: Vec<String>,
    pub can_undo: bool,
    pub can_redo: bool,
}

pub struct GameController {
    timeline: Vec<HistoryEntry>,
    cursor: usize,
}

impl GameController {
    pub fn new() -> Self {
        let initial = HistoryEntry {
            state: GameState::new(),
            last_move: None,
            last_pass: None,
            move_records: Vec::new(),
        };
        Self {
            timeline: vec![initial],
            cursor: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn click_cell(&mut self, pos: Pos) {
        self.apply_move_with_note(pos, None);
    }

    pub fn apply_move_with_note(&mut self, pos: Pos, note: Option<String>) {
        let mut next = self.current().clone();
        let moving_player = next.state.turn();
        let result = next.state.apply_move(pos);
        if !result.applied {
            return;
        }
        let _ = (result.next_turn, result.status);

        next.last_move = Some(pos);
        next.last_pass = result.passed;
        next.move_records.push(MoveRecord {
            player: moving_player,
            pos,
            flipped: result.flipped.len(),
            note,
        });

        self.timeline.truncate(self.cursor + 1);
        self.timeline.push(next);
        self.cursor += 1;
    }

    pub fn undo(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn redo(&mut self) {
        if self.cursor + 1 < self.timeline.len() {
            self.cursor += 1;
        }
    }

    pub fn view_model(&self) -> GameViewModel {
        let current = self.current();
        let (black_score, white_score) = current.state.score();
        let game_over = current.state.status() == GameStatus::GameOver;

        let message = if game_over {
            if black_score > white_score {
                format!("Game Over: Black wins ({black_score}-{white_score})")
            } else if white_score > black_score {
                format!("Game Over: White wins ({white_score}-{black_score})")
            } else {
                format!("Game Over: Draw ({black_score}-{white_score})")
            }
        } else if let Some(passed_player) = current.last_pass {
            format!(
                "{} has no legal moves. Pass. Turn: {}",
                passed_player.name(),
                current.state.turn().name()
            )
        } else {
            format!(
                "Turn: {}  |  Black: {}  White: {}",
                current.state.turn().name(),
                black_score,
                white_score
            )
        };

        let history_lines = current
            .move_records
            .iter()
            .enumerate()
            .map(|(idx, rec)| {
                let base = format!(
                    "{:>2}. {:<5} {} (+{})",
                    idx + 1,
                    rec.player.name(),
                    rec.pos.notation(),
                    rec.flipped
                );
                if let Some(note) = &rec.note {
                    format!("{base}  [{note}]")
                } else {
                    base
                }
            })
            .collect();

        GameViewModel {
            board: *current.state.board().cells(),
            legal_moves: current.state.legal_moves(current.state.turn()),
            turn: current.state.turn(),
            black_score,
            white_score,
            message,
            game_over,
            last_move: current.last_move,
            history_lines,
            can_undo: self.cursor > 0,
            can_redo: self.cursor + 1 < self.timeline.len(),
        }
    }

    fn current(&self) -> &HistoryEntry {
        &self.timeline[self.cursor]
    }
}
