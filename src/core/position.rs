#![allow(dead_code)]

use crate::core::mv::Move;
use crate::core::types::{Bitboard, Color, Piece, PieceKind, Square};
use crate::core::zobrist;
use std::{error::Error, fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub pieces: [Bitboard; 12],     // one bitboard per piece
    pub colors: [Bitboard; 2],      // bitboards for all white and all black pieces
    pub occupied: Bitboard,         // tracking all occupied squares
    pub board: [Option<Piece>; 64], // square to piece mapping

    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub ep_square: Option<Square>, // for tracking en passant square
    pub halfmove_clock: u16,
    pub fullmove_number: u16,
    pub king_sq: [Square; 2],
    pub zobrist: u64,
}

impl Position {
    pub fn empty() -> Self {
        Position {
            pieces: [Bitboard::EMPTY; 12],
            colors: [Bitboard::EMPTY; 2],
            occupied: Bitboard::EMPTY,
            board: [None; 64],
            side_to_move: Color::White,
            castling: CastlingRights::EMPTY,
            ep_square: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            king_sq: [Square::default(); 2],
            zobrist: 0,
        }
    }

    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        fen.parse()
    }

    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        self.board[sq.idx()]
    }

    pub fn color_bb(&self, c: Color) -> Bitboard {
        self.colors[c.idx()]
    }

    pub fn piece_bb(&self, p: Piece) -> Bitboard {
        self.pieces[p.idx()]
    }

    pub fn pieces_of(&self, c: Color, k: PieceKind) -> Bitboard {
        self.pieces[Piece::from(c, k).idx()]
    }

    pub fn occupied(&self) -> Bitboard {
        self.occupied
    }

    pub fn repetition_count(&self, history: &[State]) -> usize {
        1 + history
            .iter()
            .rev()
            .take(self.halfmove_clock as usize)
            .filter(|state| state.zobrist == self.zobrist)
            .count()
    }

    pub fn is_threefold_repetition(&self, history: &[State]) -> bool {
        self.repetition_count(history) >= 3
    }

    pub fn add_piece(&mut self, piece: Piece, sq: Square) {
        self.board[sq.idx()] = Some(piece);
        self.pieces[piece.idx()].set(sq);
        self.colors[piece.color().idx()].set(sq);
        self.occupied.set(sq);
        self.zobrist ^= zobrist::piece_square_key(piece.idx(), sq.idx());
        if piece.kind() == PieceKind::King {
            self.king_sq[piece.color().idx()] = sq;
        }
    }

    pub fn remove_piece(&mut self, piece: Piece, sq: Square) {
        self.board[sq.idx()] = None;
        self.pieces[piece.idx()].clear(sq);
        self.colors[piece.color().idx()].clear(sq);
        self.occupied.clear(sq);
        self.zobrist ^= zobrist::piece_square_key(piece.idx(), sq.idx());
    }

    pub fn move_piece(&mut self, piece: Piece, from: Square, to: Square) {
        self.board[from.idx()] = None;
        self.board[to.idx()] = Some(piece);

        self.pieces[piece.idx()].clear(from);
        self.pieces[piece.idx()].set(to);

        self.colors[piece.color().idx()].clear(from);
        self.colors[piece.color().idx()].set(to);

        self.occupied.clear(from);
        self.occupied.set(to);
        self.zobrist ^= zobrist::piece_square_key(piece.idx(), from.idx());
        self.zobrist ^= zobrist::piece_square_key(piece.idx(), to.idx());

        if piece.kind() == PieceKind::King {
            self.king_sq[piece.color().idx()] = to;
        }
    }

    pub fn make_move(&mut self, mv: Move) -> State {
        let from = mv.from();
        let to = mv.to();
        let piece = self
            .piece_at(from)
            .expect("cannot make a move from an empty square");
        let capture_square = if mv.is_en_passant() {
            en_passant_capture_square(mv)
        } else {
            to
        };
        let captured = if mv.is_capture() {
            self.piece_at(capture_square)
        } else {
            None
        };
        let state = State {
            castling: self.castling,
            ep_square: self.ep_square,
            halfmove_clock: self.halfmove_clock,
            captured,
            zobrist: self.zobrist,
        };

        self.set_ep_square(None);

        if let Some(captured) = captured {
            self.remove_piece(captured, capture_square);
        }

        if let Some(promoted) = mv.promotion() {
            self.remove_piece(piece, from);
            self.add_piece(Piece::from(piece.color(), promoted), to);
        } else {
            self.move_piece(piece, from, to);
            if mv.is_castling() {
                let (rook_from, rook_to) = castling_rook_squares(mv);
                self.move_piece(
                    Piece::from(piece.color(), PieceKind::Rook),
                    rook_from,
                    rook_to,
                );
            }
        }

        self.update_castling_rights_after_move(piece, from, captured, capture_square);

        let next_ep_square = if mv.is_double_pawn_push() {
            let ep_rank = (from.rank() + to.rank()) / 2;
            Some(Square::from_file_rank(from.file(), ep_rank))
        } else {
            None
        };

        if piece.kind() == PieceKind::Pawn || captured.is_some() {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }

        if self.side_to_move == Color::Black {
            self.fullmove_number += 1;
        }
        self.set_side_to_move(self.side_to_move.opposit());
        self.set_ep_square(next_ep_square);

        state
    }

    pub fn unmake_move(&mut self, mv: Move, state: State) {
        let from = mv.from();
        let to = mv.to();

        self.side_to_move = self.side_to_move.opposit();
        if self.side_to_move == Color::Black {
            self.fullmove_number -= 1;
        }

        let moving_color = self.side_to_move;
        if mv.is_castling() {
            let (rook_from, rook_to) = castling_rook_squares(mv);
            self.move_piece(
                Piece::from(moving_color, PieceKind::Rook),
                rook_to,
                rook_from,
            );
            self.move_piece(Piece::from(moving_color, PieceKind::King), to, from);
        } else if mv.promotion().is_some() {
            let promoted_piece = self
                .piece_at(to)
                .expect("cannot unmake a promotion with no promoted piece on the destination");
            self.remove_piece(promoted_piece, to);
            self.add_piece(Piece::from(moving_color, PieceKind::Pawn), from);
        } else {
            let moved_piece = self
                .piece_at(to)
                .expect("cannot unmake a move with no piece on the destination square");
            self.move_piece(moved_piece, to, from);
        }

        if let Some(captured) = state.captured {
            let capture_square = if mv.is_en_passant() {
                en_passant_capture_square(mv)
            } else {
                to
            };
            self.add_piece(captured, capture_square);
        }

        self.castling = state.castling;
        self.ep_square = state.ep_square;
        self.halfmove_clock = state.halfmove_clock;
        self.zobrist = state.zobrist;
    }

    fn update_castling_rights_after_move(
        &mut self,
        piece: Piece,
        from: Square,
        captured: Option<Piece>,
        capture_square: Square,
    ) {
        if piece.kind() == PieceKind::King {
            self.remove_castling_rights_for_color(piece.color());
        }
        if piece.kind() == PieceKind::Rook {
            self.remove_castling_right_for_rook_square(from);
        }
        if captured.is_some_and(|piece| piece.kind() == PieceKind::Rook) {
            self.remove_castling_right_for_rook_square(capture_square);
        }
    }

    fn remove_castling_rights_for_color(&mut self, color: Color) {
        let mut castling = self.castling;
        match color {
            Color::White => {
                castling.remove(CastlingRights::WHITE_KINGSIDE | CastlingRights::WHITE_QUEENSIDE)
            }
            Color::Black => {
                castling.remove(CastlingRights::BLACK_KINGSIDE | CastlingRights::BLACK_QUEENSIDE)
            }
        }
        self.set_castling(castling);
    }

    fn remove_castling_right_for_rook_square(&mut self, square: Square) {
        let mask = match (square.file(), square.rank()) {
            (0, 0) => CastlingRights::WHITE_QUEENSIDE,
            (7, 0) => CastlingRights::WHITE_KINGSIDE,
            (0, 7) => CastlingRights::BLACK_QUEENSIDE,
            (7, 7) => CastlingRights::BLACK_KINGSIDE,
            _ => 0,
        };
        let mut castling = self.castling;
        castling.remove(mask);
        self.set_castling(castling);
    }

    fn set_side_to_move(&mut self, side_to_move: Color) {
        if self.side_to_move != side_to_move {
            if let Some(old) = zobrist::effective_ep_file(self) {
                self.zobrist ^= zobrist::en_passant_file_key(old);
            }
            self.zobrist ^= zobrist::side_to_move_key();
            self.side_to_move = side_to_move;
            if let Some(new) = zobrist::effective_ep_file(self) {
                self.zobrist ^= zobrist::en_passant_file_key(new);
            }
        }
    }

    fn set_castling(&mut self, castling: CastlingRights) {
        if self.castling != castling {
            self.zobrist ^= zobrist::castling_key(self.castling);
            self.castling = castling;
            self.zobrist ^= zobrist::castling_key(self.castling);
        }
    }

    fn set_ep_square(&mut self, ep_square: Option<Square>) {
        if self.ep_square != ep_square {
            if let Some(old) = zobrist::effective_ep_file(self) {
                self.zobrist ^= zobrist::en_passant_file_key(old);
            }
            self.ep_square = ep_square;
            if let Some(new) = zobrist::effective_ep_file(self) {
                self.zobrist ^= zobrist::en_passant_file_key(new);
            }
        }
    }
}

fn en_passant_capture_square(mv: Move) -> Square {
    Square::from_file_rank(mv.to().file(), mv.from().rank())
}

fn castling_rook_squares(mv: Move) -> (Square, Square) {
    let rank = mv.from().rank();
    match mv.to().file() {
        6 => (
            Square::from_file_rank(7, rank),
            Square::from_file_rank(5, rank),
        ),
        2 => (
            Square::from_file_rank(0, rank),
            Square::from_file_rank(3, rank),
        ),
        _ => panic!("invalid castling move destination"),
    }
}

impl FromStr for Position {
    type Err = FenError;

    fn from_str(fen: &str) -> Result<Self, Self::Err> {
        let mut fields = fen.split_whitespace();
        let board = fields.next().ok_or(FenError::WrongFieldCount)?;
        let side_to_move = fields.next().ok_or(FenError::WrongFieldCount)?;
        let castling = fields.next().ok_or(FenError::WrongFieldCount)?;
        let ep_square = fields.next().ok_or(FenError::WrongFieldCount)?;
        let halfmove_clock = fields.next().ok_or(FenError::WrongFieldCount)?;
        let fullmove_number = fields.next().ok_or(FenError::WrongFieldCount)?;

        if fields.next().is_some() {
            return Err(FenError::WrongFieldCount);
        }

        let mut pos = Position::empty();
        parse_board(board, &mut pos)?;
        pos.side_to_move = parse_side_to_move(side_to_move)?;
        pos.castling = parse_castling(castling)?;
        pos.ep_square = parse_ep_square(ep_square, pos.side_to_move)?;
        pos.halfmove_clock = halfmove_clock
            .parse()
            .map_err(|_| FenError::InvalidHalfmoveClock)?;
        pos.fullmove_number = fullmove_number
            .parse()
            .map_err(|_| FenError::InvalidFullmoveNumber)?;

        if pos.fullmove_number == 0 {
            return Err(FenError::InvalidFullmoveNumber);
        }

        validate_kings(&pos)?;
        pos.zobrist = zobrist::hash_position(&pos);

        Ok(pos)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenError {
    WrongFieldCount,
    InvalidBoard,
    InvalidSideToMove,
    InvalidCastling,
    InvalidEnPassant,
    InvalidHalfmoveClock,
    InvalidFullmoveNumber,
    MissingKing(Color),
    TooManyKings(Color),
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenError::WrongFieldCount => write!(f, "FEN must contain exactly six fields"),
            FenError::InvalidBoard => write!(f, "FEN board field is invalid"),
            FenError::InvalidSideToMove => write!(f, "FEN side-to-move field is invalid"),
            FenError::InvalidCastling => write!(f, "FEN castling field is invalid"),
            FenError::InvalidEnPassant => write!(f, "FEN en passant field is invalid"),
            FenError::InvalidHalfmoveClock => write!(f, "FEN halfmove clock is invalid"),
            FenError::InvalidFullmoveNumber => write!(f, "FEN fullmove number is invalid"),
            FenError::MissingKing(color) => write!(f, "FEN is missing a {color:?} king"),
            FenError::TooManyKings(color) => write!(f, "FEN has more than one {color:?} king"),
        }
    }
}

impl Error for FenError {}

fn parse_board(board: &str, pos: &mut Position) -> Result<(), FenError> {
    let mut rank_count = 0;
    for (fen_rank, rank_str) in board.split('/').enumerate() {
        if fen_rank >= 8 {
            return Err(FenError::InvalidBoard);
        }

        let rank = 7 - fen_rank as u8;
        let mut file = 0;

        for ch in rank_str.chars() {
            if let Some(empty_squares) = ch.to_digit(10) {
                if empty_squares == 0 || empty_squares > 8 {
                    return Err(FenError::InvalidBoard);
                }
                file += empty_squares as u8;
                if file > 8 {
                    return Err(FenError::InvalidBoard);
                }
                continue;
            }

            if file >= 8 {
                return Err(FenError::InvalidBoard);
            }

            let piece = piece_from_fen(ch).ok_or(FenError::InvalidBoard)?;
            pos.add_piece(piece, Square::from_file_rank(file, rank));
            file += 1;
        }

        if file != 8 {
            return Err(FenError::InvalidBoard);
        }

        rank_count += 1;
    }

    if rank_count == 8 {
        Ok(())
    } else {
        Err(FenError::InvalidBoard)
    }
}

fn piece_from_fen(ch: char) -> Option<Piece> {
    match ch {
        'P' => Some(Piece::WhitePawn),
        'N' => Some(Piece::WhiteKnight),
        'B' => Some(Piece::WhiteBishop),
        'R' => Some(Piece::WhiteRook),
        'Q' => Some(Piece::WhiteQueen),
        'K' => Some(Piece::WhiteKing),
        'p' => Some(Piece::BlackPawn),
        'n' => Some(Piece::BlackKnight),
        'b' => Some(Piece::BlackBishop),
        'r' => Some(Piece::BlackRook),
        'q' => Some(Piece::BlackQueen),
        'k' => Some(Piece::BlackKing),
        _ => None,
    }
}

fn parse_side_to_move(side_to_move: &str) -> Result<Color, FenError> {
    match side_to_move {
        "w" => Ok(Color::White),
        "b" => Ok(Color::Black),
        _ => Err(FenError::InvalidSideToMove),
    }
}

fn parse_castling(castling: &str) -> Result<CastlingRights, FenError> {
    if castling == "-" {
        return Ok(CastlingRights::EMPTY);
    }

    let mut rights = CastlingRights::EMPTY;

    for ch in castling.chars() {
        let mask = match ch {
            'K' => CastlingRights::WHITE_KINGSIDE,
            'Q' => CastlingRights::WHITE_QUEENSIDE,
            'k' => CastlingRights::BLACK_KINGSIDE,
            'q' => CastlingRights::BLACK_QUEENSIDE,
            _ => return Err(FenError::InvalidCastling),
        };

        if rights.has(mask) {
            return Err(FenError::InvalidCastling);
        }
        rights.insert(mask);
    }

    Ok(rights)
}

fn parse_ep_square(ep_square: &str, side_to_move: Color) -> Result<Option<Square>, FenError> {
    if ep_square == "-" {
        return Ok(None);
    }

    let bytes = ep_square.as_bytes();
    if bytes.len() != 2 {
        return Err(FenError::InvalidEnPassant);
    }

    let file = match bytes[0] {
        b'a'..=b'h' => bytes[0] - b'a',
        _ => return Err(FenError::InvalidEnPassant),
    };
    let rank = match bytes[1] {
        b'1'..=b'8' => bytes[1] - b'1',
        _ => return Err(FenError::InvalidEnPassant),
    };

    let expected_rank = match side_to_move {
        Color::White => 5,
        Color::Black => 2,
    };
    if rank != expected_rank {
        return Err(FenError::InvalidEnPassant);
    }

    Ok(Some(Square::from_file_rank(file, rank)))
}

fn validate_kings(pos: &Position) -> Result<(), FenError> {
    for color in [Color::White, Color::Black] {
        match pos.pieces_of(color, PieceKind::King).count() {
            0 => return Err(FenError::MissingKing(color)),
            1 => {}
            _ => return Err(FenError::TooManyKings(color)),
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct State {
    pub castling: CastlingRights,
    pub ep_square: Option<Square>,
    pub halfmove_clock: u16,
    pub captured: Option<Piece>,
    pub zobrist: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct CastlingRights(pub u8);

impl CastlingRights {
    pub const EMPTY: Self = CastlingRights(0);
    pub const WHITE_KINGSIDE: u8 = 0b0001;
    pub const WHITE_QUEENSIDE: u8 = 0b0010;
    pub const BLACK_KINGSIDE: u8 = 0b0100;
    pub const BLACK_QUEENSIDE: u8 = 0b1000;

    pub const fn has(self, mask: u8) -> bool {
        self.0 & mask != 0
    }

    pub const fn remove(&mut self, mask: u8) {
        self.0 &= !mask;
    }

    pub const fn insert(&mut self, mask: u8) {
        self.0 |= mask;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::zobrist;

    const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    fn assert_hash_consistent(pos: &Position) {
        assert_eq!(pos.zobrist, zobrist::hash_position(pos));
    }

    fn assert_move_hash_consistent_after_make(fen: &str, mv: Move) {
        let mut pos = Position::from_fen(fen).unwrap();
        let before = pos.clone();
        let before_hash = pos.zobrist;

        let state = pos.make_move(mv);
        assert_hash_consistent(&pos);

        pos.unmake_move(mv, state);
        assert_eq!(pos, before);
        assert_eq!(pos.zobrist, before_hash);
        assert_hash_consistent(&pos);
    }

    fn record_move(pos: &mut Position, history: &mut Vec<State>, mv: Move) {
        let state = pos.make_move(mv);
        history.push(state);
    }

    fn state_with_zobrist(zobrist: u64) -> State {
        State {
            castling: CastlingRights::EMPTY,
            ep_square: None,
            halfmove_clock: 0,
            captured: None,
            zobrist,
        }
    }

    fn record_knight_cycle(pos: &mut Position, history: &mut Vec<State>) {
        record_move(
            pos,
            history,
            Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2)),
        );
        record_move(
            pos,
            history,
            Move::new(Square::from_file_rank(6, 7), Square::from_file_rank(5, 5)),
        );
        record_move(
            pos,
            history,
            Move::new(Square::from_file_rank(5, 2), Square::from_file_rank(6, 0)),
        );
        record_move(
            pos,
            history,
            Move::new(Square::from_file_rank(5, 5), Square::from_file_rank(6, 7)),
        );
    }

    #[test]
    fn parse_starting_position() {
        let pos = Position::from_fen(STARTPOS).unwrap();

        assert_eq!(pos.side_to_move, Color::White);
        assert_eq!(
            pos.piece_at(Square::from_file_rank(0, 0)),
            Some(Piece::WhiteRook)
        );
        assert_eq!(
            pos.piece_at(Square::from_file_rank(4, 0)),
            Some(Piece::WhiteKing)
        );
        assert_eq!(
            pos.piece_at(Square::from_file_rank(4, 7)),
            Some(Piece::BlackKing)
        );
        assert_eq!(
            pos.piece_at(Square::from_file_rank(7, 7)),
            Some(Piece::BlackRook)
        );
        assert_eq!(pos.pieces_of(Color::White, PieceKind::Pawn).count(), 8);
        assert_eq!(pos.pieces_of(Color::Black, PieceKind::Pawn).count(), 8);
        assert_eq!(pos.color_bb(Color::White).count(), 16);
        assert_eq!(pos.color_bb(Color::Black).count(), 16);
        assert_eq!(pos.occupied().count(), 32);
        assert_eq!(
            pos.king_sq[Color::White.idx()],
            Square::from_file_rank(4, 0)
        );
        assert_eq!(
            pos.king_sq[Color::Black.idx()],
            Square::from_file_rank(4, 7)
        );
        assert!(pos.castling.has(CastlingRights::WHITE_KINGSIDE));
        assert!(pos.castling.has(CastlingRights::WHITE_QUEENSIDE));
        assert!(pos.castling.has(CastlingRights::BLACK_KINGSIDE));
        assert!(pos.castling.has(CastlingRights::BLACK_QUEENSIDE));
        assert_eq!(pos.ep_square, None);
        assert_eq!(pos.halfmove_clock, 0);
        assert_eq!(pos.fullmove_number, 1);
    }

    #[test]
    fn repetition_count_is_one_without_history() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();

        assert_eq!(pos.repetition_count(&[]), 1);
        assert!(!pos.is_threefold_repetition(&[]));
    }

    #[test]
    fn repetition_count_includes_one_prior_reversible_cycle() {
        let mut pos = Position::from_fen("4k1n1/8/8/8/8/8/8/4K1N1 w - - 0 1").unwrap();
        let mut history = Vec::new();

        record_knight_cycle(&mut pos, &mut history);

        assert_eq!(pos.repetition_count(&history), 2);
        assert!(!pos.is_threefold_repetition(&history));
    }

    #[test]
    fn repetition_count_detects_threefold_after_two_reversible_cycles() {
        let mut pos = Position::from_fen("4k1n1/8/8/8/8/8/8/4K1N1 w - - 0 1").unwrap();
        let mut history = Vec::new();

        record_knight_cycle(&mut pos, &mut history);
        record_knight_cycle(&mut pos, &mut history);

        assert_eq!(pos.repetition_count(&history), 3);
        assert!(pos.is_threefold_repetition(&history));
    }

    #[test]
    fn repetition_count_ignores_matches_before_pawn_move() {
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/P7/4K3 w - - 7 1").unwrap();
        let pawn_move = Move::new(Square::from_file_rank(0, 1), Square::from_file_rank(0, 2));
        let state = pos.make_move(pawn_move);
        let stale_match = state_with_zobrist(pos.zobrist);
        let history = vec![stale_match, state];

        assert_eq!(pos.halfmove_clock, 0);
        assert_eq!(pos.repetition_count(&history), 1);
        assert!(!pos.is_threefold_repetition(&history));
    }

    #[test]
    fn repetition_count_ignores_matches_before_capture() {
        let mut pos = Position::from_fen("4k3/8/8/8/8/5p2/8/4K1N1 w - - 7 1").unwrap();
        let capture_move =
            Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2)).with_capture();
        let state = pos.make_move(capture_move);
        let stale_match = state_with_zobrist(pos.zobrist);
        let history = vec![stale_match, state];

        assert_eq!(pos.halfmove_clock, 0);
        assert_eq!(pos.repetition_count(&history), 1);
        assert!(!pos.is_threefold_repetition(&history));
    }

    #[test]
    fn repetition_count_for_fen_without_history_is_one_with_nonzero_halfmove_clock() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 42 1").unwrap();

        assert_eq!(pos.repetition_count(&[]), 1);
        assert!(!pos.is_threefold_repetition(&[]));
    }

    #[test]
    fn parse_position_metadata() {
        let pos = Position::from_fen("4k3/8/8/8/3pP3/8/8/4K3 b Kq e3 17 42").unwrap();

        assert_eq!(pos.side_to_move, Color::Black);
        assert_eq!(pos.castling, CastlingRights(0b1001));
        assert_eq!(pos.ep_square, Some(Square::from_file_rank(4, 2)));
        assert_eq!(pos.halfmove_clock, 17);
        assert_eq!(pos.fullmove_number, 42);
    }

    #[test]
    fn parse_from_str() {
        let pos: Position = "4k3/8/8/8/8/8/8/4K3 w - - 0 1".parse().unwrap();

        assert_eq!(
            pos.piece_at(Square::from_file_rank(4, 0)),
            Some(Piece::WhiteKing)
        );
        assert_eq!(
            pos.piece_at(Square::from_file_rank(4, 7)),
            Some(Piece::BlackKing)
        );
    }

    #[test]
    fn reject_wrong_field_count() {
        assert_eq!(
            Position::from_fen("8/8/8/8/8/8/8/8 w - - 0").unwrap_err(),
            FenError::WrongFieldCount
        );
    }

    #[test]
    fn reject_bad_board_shape() {
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/9K w - - 0 1").unwrap_err(),
            FenError::InvalidBoard
        );
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/7 w - - 0 1").unwrap_err(),
            FenError::InvalidBoard
        );
    }

    #[test]
    fn reject_bad_metadata() {
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/4K3 x - - 0 1").unwrap_err(),
            FenError::InvalidSideToMove
        );
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/4K3 w KK - 0 1").unwrap_err(),
            FenError::InvalidCastling
        );
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - e4 0 1").unwrap_err(),
            FenError::InvalidEnPassant
        );
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - x 1").unwrap_err(),
            FenError::InvalidHalfmoveClock
        );
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 0").unwrap_err(),
            FenError::InvalidFullmoveNumber
        );
    }

    #[test]
    fn reject_missing_or_extra_kings() {
        assert_eq!(
            Position::from_fen("8/8/8/8/8/8/8/4K3 w - - 0 1").unwrap_err(),
            FenError::MissingKing(Color::Black)
        );
        assert_eq!(
            Position::from_fen("4k3/8/8/8/8/8/8/4KK2 w - - 0 1").unwrap_err(),
            FenError::TooManyKings(Color::White)
        );
    }

    #[test]
    fn piece_mutators_keep_hash_consistent() {
        let mut pos = Position::empty();
        assert_hash_consistent(&pos);

        pos.add_piece(Piece::WhiteKing, Square::from_file_rank(4, 0));
        assert_hash_consistent(&pos);

        pos.add_piece(Piece::BlackKing, Square::from_file_rank(4, 7));
        assert_hash_consistent(&pos);

        pos.add_piece(Piece::WhiteKnight, Square::from_file_rank(6, 0));
        assert_hash_consistent(&pos);

        pos.move_piece(
            Piece::WhiteKnight,
            Square::from_file_rank(6, 0),
            Square::from_file_rank(5, 2),
        );
        assert_hash_consistent(&pos);

        pos.remove_piece(Piece::WhiteKnight, Square::from_file_rank(5, 2));
        assert_hash_consistent(&pos);
    }

    #[test]
    fn make_move_keeps_hash_consistent_for_common_metadata_changes() {
        let cases = [
            (
                "4k3/8/8/8/8/8/8/4K1N1 w - - 4 1",
                Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2)),
            ),
            (
                "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
                Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3))
                    .with_double_pawn_push(),
            ),
            (
                "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
                Move::new(Square::from_file_rank(4, 0), Square::from_file_rank(6, 0))
                    .with_castling(),
            ),
            (
                "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
                Move::new(Square::from_file_rank(7, 0), Square::from_file_rank(7, 1)),
            ),
            (
                "4k2r/8/8/8/8/8/8/4K2R w Kk - 0 1",
                Move::new(Square::from_file_rank(7, 0), Square::from_file_rank(7, 7))
                    .with_capture(),
            ),
            (
                "4k1n1/8/8/8/8/8/8/4K3 b - - 4 7",
                Move::new(Square::from_file_rank(6, 7), Square::from_file_rank(5, 5)),
            ),
        ];

        for (fen, mv) in cases {
            assert_move_hash_consistent_after_make(fen, mv);
        }
    }

    #[test]
    fn quiet_move_make_unmake_restores_position() {
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/8/4K1N1 w - - 4 1").unwrap();
        let before = pos.clone();
        let mv = Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2));

        let state = pos.make_move(mv);
        pos.unmake_move(mv, state);

        assert_eq!(pos, before);
    }

    #[test]
    fn capture_make_unmake_restores_position() {
        let mut pos = Position::from_fen("4k3/8/8/8/8/5p2/8/4K1N1 w - - 4 1").unwrap();
        let before = pos.clone();
        let before_hash = pos.zobrist;
        let mv =
            Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2)).with_capture();

        let state = pos.make_move(mv);
        pos.unmake_move(mv, state);

        assert_eq!(pos, before);
        assert_eq!(pos.zobrist, before_hash);
        assert_hash_consistent(&pos);
    }

    #[test]
    fn double_pawn_push_sets_en_passant_square() {
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let mv = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3))
            .with_double_pawn_push();

        let state = pos.make_move(mv);

        assert_eq!(pos.ep_square, Some(Square::from_file_rank(4, 2)));
        pos.unmake_move(mv, state);
        assert_eq!(pos.ep_square, None);
    }

    #[test]
    fn halfmove_clock_updates_for_normal_pawn_and_capture_moves() {
        let mut quiet = Position::from_fen("4k3/8/8/8/8/8/8/4K1N1 w - - 4 1").unwrap();
        let quiet_mv = Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2));
        let quiet_state = quiet.make_move(quiet_mv);
        assert_eq!(quiet.halfmove_clock, 5);
        quiet.unmake_move(quiet_mv, quiet_state);
        assert_eq!(quiet.halfmove_clock, 4);

        let mut pawn = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 4 1").unwrap();
        let pawn_mv = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 2));
        let pawn_state = pawn.make_move(pawn_mv);
        assert_eq!(pawn.halfmove_clock, 0);
        pawn.unmake_move(pawn_mv, pawn_state);
        assert_eq!(pawn.halfmove_clock, 4);

        let mut capture = Position::from_fen("4k3/8/8/8/8/5p2/8/4K1N1 w - - 4 1").unwrap();
        let capture_mv =
            Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2)).with_capture();
        let capture_state = capture.make_move(capture_mv);
        assert_eq!(capture.halfmove_clock, 0);
        capture.unmake_move(capture_mv, capture_state);
        assert_eq!(capture.halfmove_clock, 4);
    }

    #[test]
    fn black_move_increments_fullmove_and_unmake_restores_it() {
        let mut pos = Position::from_fen("4k1n1/8/8/8/8/8/8/4K3 b - - 4 7").unwrap();
        let before = pos.clone();
        let mv = Move::new(Square::from_file_rank(6, 7), Square::from_file_rank(5, 5));

        let state = pos.make_move(mv);

        assert_eq!(pos.fullmove_number, 8);
        pos.unmake_move(mv, state);
        assert_eq!(pos, before);
    }

    #[test]
    fn promotion_make_unmake_restores_position_for_each_piece() {
        for promoted in [
            PieceKind::Knight,
            PieceKind::Bishop,
            PieceKind::Rook,
            PieceKind::Queen,
        ] {
            let mut pos = Position::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").unwrap();
            let before = pos.clone();
            let before_hash = pos.zobrist;
            let mv = Move::with_promotion(
                Square::from_file_rank(0, 6),
                Square::from_file_rank(0, 7),
                promoted,
            );

            let state = pos.make_move(mv);

            assert_eq!(
                pos.piece_at(Square::from_file_rank(0, 7)),
                Some(Piece::from(Color::White, promoted))
            );
            assert_eq!(pos.piece_at(Square::from_file_rank(0, 6)), None);
            pos.unmake_move(mv, state);
            assert_eq!(pos, before);
            assert_eq!(pos.zobrist, before_hash);
            assert_hash_consistent(&pos);
        }
    }

    #[test]
    fn en_passant_make_unmake_restores_position() {
        let mut pos = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        let before = pos.clone();
        let before_hash = pos.zobrist;
        let mv =
            Move::new(Square::from_file_rank(4, 4), Square::from_file_rank(3, 5)).with_en_passant();

        let state = pos.make_move(mv);

        assert_eq!(
            pos.piece_at(Square::from_file_rank(3, 5)),
            Some(Piece::WhitePawn)
        );
        assert_eq!(pos.piece_at(Square::from_file_rank(3, 4)), None);
        pos.unmake_move(mv, state);
        assert_eq!(pos, before);
        assert_eq!(pos.zobrist, before_hash);
        assert_hash_consistent(&pos);
    }

    #[test]
    fn all_castling_moves_make_unmake_restore_position() {
        let cases = [
            (
                "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
                Square::from_file_rank(4, 0),
                Square::from_file_rank(6, 0),
                Square::from_file_rank(5, 0),
                Piece::WhiteRook,
            ),
            (
                "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
                Square::from_file_rank(4, 0),
                Square::from_file_rank(2, 0),
                Square::from_file_rank(3, 0),
                Piece::WhiteRook,
            ),
            (
                "r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1",
                Square::from_file_rank(4, 7),
                Square::from_file_rank(6, 7),
                Square::from_file_rank(5, 7),
                Piece::BlackRook,
            ),
            (
                "r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1",
                Square::from_file_rank(4, 7),
                Square::from_file_rank(2, 7),
                Square::from_file_rank(3, 7),
                Piece::BlackRook,
            ),
        ];

        for (fen, from, to, rook_to, rook) in cases {
            let mut pos = Position::from_fen(fen).unwrap();
            let before = pos.clone();
            let mv = Move::new(from, to).with_castling();

            let state = pos.make_move(mv);

            assert_eq!(pos.piece_at(to).unwrap().kind(), PieceKind::King);
            assert_eq!(pos.piece_at(rook_to), Some(rook));
            pos.unmake_move(mv, state);
            assert_eq!(pos, before);
        }
    }

    #[test]
    fn king_move_removes_both_castling_rights_for_color() {
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/4K3/R6R w KQ - 0 1").unwrap();
        let mv = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 2));

        pos.make_move(mv);

        assert!(!pos.castling.has(CastlingRights::WHITE_KINGSIDE));
        assert!(!pos.castling.has(CastlingRights::WHITE_QUEENSIDE));
    }

    #[test]
    fn rook_move_removes_only_matching_castling_right() {
        let mut pos = Position::from_fen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1").unwrap();
        let mv = Move::new(Square::from_file_rank(7, 0), Square::from_file_rank(7, 1));

        pos.make_move(mv);

        assert!(!pos.castling.has(CastlingRights::WHITE_KINGSIDE));
        assert!(pos.castling.has(CastlingRights::WHITE_QUEENSIDE));
    }

    #[test]
    fn rook_capture_on_original_square_removes_matching_opponent_right() {
        let mut pos = Position::from_fen("4k2r/8/8/8/8/8/8/4K2R w Kk - 0 1").unwrap();
        let mv =
            Move::new(Square::from_file_rank(7, 0), Square::from_file_rank(7, 7)).with_capture();

        pos.make_move(mv);

        assert!(!pos.castling.has(CastlingRights::BLACK_KINGSIDE));
    }
}
