pub const BOARD_SIZE: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cell {
    Empty,
    Black,
    White,
}

impl Cell {
    pub fn opposite(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
            Self::Empty => Self::Empty,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Black => "Black",
            Self::White => "White",
            Self::Empty => "Empty",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Pos {
    pub x: usize,
    pub y: usize,
}

impl Pos {
    pub fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }

    pub fn notation(self) -> String {
        let file = (b'a' + self.x as u8) as char;
        let rank = self.y + 1;
        format!("{file}{rank}")
    }
}

#[derive(Clone, Debug)]
pub struct Board {
    cells: [[Cell; BOARD_SIZE]; BOARD_SIZE],
}

impl Board {
    pub fn new() -> Self {
        Self {
            cells: [[Cell::Empty; BOARD_SIZE]; BOARD_SIZE],
        }
    }

    pub fn get(&self, pos: Pos) -> Cell {
        self.cells[pos.y][pos.x]
    }

    pub fn set(&mut self, pos: Pos, cell: Cell) {
        self.cells[pos.y][pos.x] = cell;
    }

    pub fn cells(&self) -> &[[Cell; BOARD_SIZE]; BOARD_SIZE] {
        &self.cells
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameStatus {
    InProgress,
    GameOver,
}

#[derive(Clone, Debug)]
pub struct MoveResult {
    pub applied: bool,
    pub flipped: Vec<Pos>,
    pub next_turn: Cell,
    pub passed: Option<Cell>,
    pub status: GameStatus,
}

#[derive(Clone, Debug)]
pub struct GameState {
    board: Board,
    turn: Cell,
    status: GameStatus,
}

impl GameState {
    pub fn new() -> Self {
        let mut board = Board::new();
        board.set(Pos::new(3, 3), Cell::White);
        board.set(Pos::new(4, 3), Cell::Black);
        board.set(Pos::new(3, 4), Cell::Black);
        board.set(Pos::new(4, 4), Cell::White);

        Self {
            board,
            turn: Cell::Black,
            status: GameStatus::InProgress,
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn turn(&self) -> Cell {
        self.turn
    }

    pub fn status(&self) -> GameStatus {
        self.status
    }

    pub fn score(&self) -> (usize, usize) {
        let mut black = 0usize;
        let mut white = 0usize;
        for row in self.board.cells() {
            for cell in row {
                match cell {
                    Cell::Black => black += 1,
                    Cell::White => white += 1,
                    Cell::Empty => {}
                }
            }
        }
        (black, white)
    }

    pub fn legal_moves(&self, player: Cell) -> Vec<Pos> {
        if self.status == GameStatus::GameOver {
            return Vec::new();
        }
        let mut moves = Vec::new();
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                let pos = Pos::new(x, y);
                if !self.flippable(pos, player).is_empty() {
                    moves.push(pos);
                }
            }
        }
        moves
    }

    pub fn apply_move(&mut self, pos: Pos) -> MoveResult {
        if self.status == GameStatus::GameOver {
            return MoveResult {
                applied: false,
                flipped: Vec::new(),
                next_turn: self.turn,
                passed: None,
                status: self.status,
            };
        }

        let flipped = self.flippable(pos, self.turn);
        if flipped.is_empty() {
            return MoveResult {
                applied: false,
                flipped,
                next_turn: self.turn,
                passed: None,
                status: self.status,
            };
        }

        self.board.set(pos, self.turn);
        for p in &flipped {
            self.board.set(*p, self.turn);
        }

        let next = self.turn.opposite();
        let next_moves = self.legal_moves(next);
        let mut passed = None;

        if !next_moves.is_empty() {
            self.turn = next;
        } else {
            let current_moves = self.legal_moves(self.turn);
            if current_moves.is_empty() {
                self.status = GameStatus::GameOver;
            } else {
                passed = Some(next);
            }
        }

        MoveResult {
            applied: true,
            flipped,
            next_turn: self.turn,
            passed,
            status: self.status,
        }
    }

    fn flippable(&self, pos: Pos, player: Cell) -> Vec<Pos> {
        if self.board.get(pos) != Cell::Empty {
            return Vec::new();
        }

        let mut out = Vec::new();
        let dirs = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ];
        for (dx, dy) in dirs {
            out.extend(self.flippable_in_dir(pos, dx, dy, player));
        }
        out
    }

    fn flippable_in_dir(&self, origin: Pos, dx: isize, dy: isize, player: Cell) -> Vec<Pos> {
        let mut x = origin.x as isize + dx;
        let mut y = origin.y as isize + dy;
        let mut captured = Vec::new();

        while Self::in_bounds(x, y) {
            let pos = Pos::new(x as usize, y as usize);
            let cell = self.board.get(pos);
            if cell == player.opposite() {
                captured.push(pos);
            } else if cell == player {
                if captured.is_empty() {
                    return Vec::new();
                }
                return captured;
            } else {
                return Vec::new();
            }
            x += dx;
            y += dy;
        }
        Vec::new()
    }

    fn in_bounds(x: isize, y: isize) -> bool {
        x >= 0 && x < BOARD_SIZE as isize && y >= 0 && y < BOARD_SIZE as isize
    }
}
