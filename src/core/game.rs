#![allow(dead_code)]

use crate::core::movegen::generate_legal;
use crate::core::mv::Move;
use crate::core::position::{FenError, Position, State};

pub const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, Clone)]
pub struct Game {
    pub pos: Position,
    pub history: Vec<State>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameError {
    IllegalMove,
}

impl Game {
    pub fn startpos() -> Self {
        Self::from_fen(STARTPOS_FEN).expect("STARTPOS_FEN must be valid")
    }

    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        Ok(Self {
            pos: Position::from_fen(fen)?,
            history: Vec::new(),
        })
    }

    pub fn legal_moves(&mut self) -> Vec<Move> {
        let mut moves = Vec::new();
        generate_legal(&mut self.pos, &mut moves);
        moves
    }

    pub fn apply_move(&mut self, mv: Move) {
        let state = self.pos.make_move(mv);
        self.history.push(state);
    }

    pub fn apply_uci_move(&mut self, text: &str) -> Result<Move, GameError> {
        let mv = self
            .legal_moves()
            .into_iter()
            .find(|mv| mv.to_uci() == text)
            .ok_or(GameError::IllegalMove)?;

        self.apply_move(mv);
        Ok(mv)
    }

    pub fn repetition_count(&self) -> usize {
        self.pos.repetition_count(&self.history)
    }

    pub fn is_threefold_repetition(&self) -> bool {
        self.pos.is_threefold_repetition(&self.history)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::types::{Color, Piece, Square};

    fn record_move(game: &mut Game, mv: Move) {
        game.apply_move(mv);
    }

    fn record_knight_cycle(game: &mut Game) {
        record_move(
            game,
            Move::new(Square::from_file_rank(6, 0), Square::from_file_rank(5, 2)),
        );
        record_move(
            game,
            Move::new(Square::from_file_rank(6, 7), Square::from_file_rank(5, 5)),
        );
        record_move(
            game,
            Move::new(Square::from_file_rank(5, 2), Square::from_file_rank(6, 0)),
        );
        record_move(
            game,
            Move::new(Square::from_file_rank(5, 5), Square::from_file_rank(6, 7)),
        );
    }

    #[test]
    fn game_repetition_count_delegates_to_position() {
        let mut game = Game {
            pos: Position::from_fen("4k1n1/8/8/8/8/8/8/4K1N1 w - - 0 1").unwrap(),
            history: Vec::new(),
        };

        record_knight_cycle(&mut game);

        assert_eq!(
            game.repetition_count(),
            game.pos.repetition_count(&game.history)
        );
    }

    #[test]
    fn game_threefold_repetition_delegates_to_position() {
        let mut game = Game {
            pos: Position::from_fen("4k1n1/8/8/8/8/8/8/4K1N1 w - - 0 1").unwrap(),
            history: Vec::new(),
        };

        record_knight_cycle(&mut game);
        record_knight_cycle(&mut game);

        assert!(game.is_threefold_repetition());
        assert_eq!(
            game.is_threefold_repetition(),
            game.pos.is_threefold_repetition(&game.history)
        );
    }

    #[test]
    fn startpos_has_twenty_legal_moves() {
        let mut game = Game::startpos();

        assert_eq!(game.legal_moves().len(), 20);
    }

    #[test]
    fn apply_uci_moves_updates_position_and_history() {
        let mut game = Game::startpos();

        game.apply_uci_move("e2e4").unwrap();
        game.apply_uci_move("e7e5").unwrap();
        game.apply_uci_move("g1f3").unwrap();

        assert_eq!(game.history.len(), 3);
        assert_eq!(game.pos.side_to_move, Color::Black);
        assert_eq!(
            game.pos.piece_at(Square::from_file_rank(5, 2)),
            Some(Piece::WhiteKnight)
        );
    }

    #[test]
    fn illegal_uci_move_leaves_game_unchanged() {
        let mut game = Game::startpos();
        let before = game.pos.clone();

        assert_eq!(game.apply_uci_move("e2e5"), Err(GameError::IllegalMove));
        assert_eq!(game.pos, before);
        assert!(game.history.is_empty());
    }

    #[test]
    fn apply_uci_promotion_selects_promoted_piece() {
        let mut game = Game::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").unwrap();

        let mv = game.apply_uci_move("a7a8q").unwrap();

        assert_eq!(mv.to_uci(), "a7a8q");
        assert_eq!(
            game.pos.piece_at(Square::from_file_rank(0, 7)),
            Some(Piece::WhiteQueen)
        );
    }

    #[test]
    fn apply_uci_en_passant_when_legal() {
        let mut game = Game::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();

        let mv = game.apply_uci_move("e5d6").unwrap();

        assert!(mv.is_en_passant());
        assert_eq!(
            game.pos.piece_at(Square::from_file_rank(3, 5)),
            Some(Piece::WhitePawn)
        );
        assert_eq!(game.pos.piece_at(Square::from_file_rank(3, 4)), None);
    }
}
