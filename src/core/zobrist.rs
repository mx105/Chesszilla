#![allow(dead_code)]

use crate::core::position::{CastlingRights, Position};
use crate::core::types::{Color, Piece, PieceKind, Square};

const SEED: u64 = 0x9e37_79b9_7f4a_7c15;
const PIECE_SQUARE_OFFSET: u64 = 0;
const SIDE_TO_MOVE_OFFSET: u64 = 12 * 64;
const CASTLING_OFFSET: u64 = SIDE_TO_MOVE_OFFSET + 1;
const EN_PASSANT_OFFSET: u64 = CASTLING_OFFSET + 16;

pub fn hash_position(pos: &Position) -> u64 {
    let mut hash = 0;

    for (square_idx, piece) in pos.board.iter().enumerate() {
        if let Some(piece) = piece {
            hash ^= piece_square_key(piece.idx(), square_idx);
        }
    }

    if pos.side_to_move == Color::Black {
        hash ^= side_to_move_key();
    }

    hash ^= castling_key(pos.castling);

    if let Some(file) = effective_ep_file(pos) {
        hash ^= en_passant_file_key(file);
    }

    hash
}

pub(crate) fn effective_ep_file(pos: &Position) -> Option<u8> {
    let ep_square = pos.ep_square?;
    let pawn_rank = match pos.side_to_move {
        Color::White => ep_square.rank().checked_sub(1)?,
        Color::Black => {
            let rank = ep_square.rank() + 1;
            if rank < 8 {
                rank
            } else {
                return None;
            }
        }
    };
    let pawn = Piece::from(pos.side_to_move, PieceKind::Pawn);

    for file_delta in [-1, 1] {
        let file = ep_square.file() as i8 + file_delta;
        if (0..8).contains(&file)
            && pos.piece_at(Square::from_file_rank(file as u8, pawn_rank)) == Some(pawn)
        {
            return Some(ep_square.file());
        }
    }

    None
}

pub(crate) const fn piece_square_key(piece_idx: usize, square_idx: usize) -> u64 {
    key(PIECE_SQUARE_OFFSET + (piece_idx as u64 * 64) + square_idx as u64)
}

pub(crate) const fn side_to_move_key() -> u64 {
    key(SIDE_TO_MOVE_OFFSET)
}

pub(crate) const fn castling_key(castling: CastlingRights) -> u64 {
    if castling.0 == 0 {
        0
    } else {
        key(CASTLING_OFFSET + castling.0 as u64)
    }
}

pub(crate) const fn en_passant_file_key(file: u8) -> u64 {
    key(EN_PASSANT_OFFSET + file as u64)
}

const fn key(index: u64) -> u64 {
    splitmix64(SEED.wrapping_add(index))
}

const fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::mv::Move;
    use crate::core::position::Position;
    use crate::core::types::Square;

    const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn parsed_start_position_hash_is_nonzero() {
        let pos = Position::from_fen(STARTPOS).unwrap();

        assert_ne!(pos.zobrist, 0);
        assert_eq!(pos.zobrist, hash_position(&pos));
    }

    #[test]
    fn identical_fens_have_identical_hashes() {
        let first = Position::from_fen(STARTPOS).unwrap();
        let second = Position::from_fen(STARTPOS).unwrap();

        assert_eq!(first.zobrist, second.zobrist);
    }

    #[test]
    fn side_to_move_changes_hash() {
        let white = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let black = Position::from_fen("4k3/8/8/8/8/8/8/4K3 b - - 0 1").unwrap();

        assert_ne!(white.zobrist, black.zobrist);
    }

    #[test]
    fn castling_rights_change_hash() {
        let with_rights = Position::from_fen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1").unwrap();
        let without_rights = Position::from_fen("4k3/8/8/8/8/8/8/R3K2R w - - 0 1").unwrap();

        assert_ne!(with_rights.zobrist, without_rights.zobrist);
    }

    #[test]
    fn en_passant_state_changes_hash() {
        let with_ep = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        let without_ep = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1").unwrap();

        assert_ne!(with_ep.zobrist, without_ep.zobrist);
    }

    #[test]
    fn non_capturable_en_passant_square_does_not_change_hash() {
        let with_ep = Position::from_fen("4k3/8/8/3p4/8/8/8/4K3 w - d6 0 1").unwrap();
        let without_ep = Position::from_fen("4k3/8/8/3p4/8/8/8/4K3 w - - 0 1").unwrap();

        assert_eq!(effective_ep_file(&with_ep), None);
        assert_eq!(with_ep.zobrist, without_ep.zobrist);
    }

    #[test]
    fn capturable_en_passant_square_changes_hash() {
        let with_ep = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        let without_ep = Position::from_fen("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1").unwrap();

        assert_eq!(effective_ep_file(&with_ep), Some(3));
        assert_ne!(with_ep.zobrist, without_ep.zobrist);
    }

    #[test]
    fn double_pawn_push_hashes_only_capturable_en_passant_file() {
        let mv = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3))
            .with_double_pawn_push();
        let mut non_capturable = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let mut capturable = Position::from_fen("4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1").unwrap();

        non_capturable.make_move(mv);
        capturable.make_move(mv);

        let mut non_capturable_without_ep = non_capturable.clone();
        non_capturable_without_ep.ep_square = None;
        non_capturable_without_ep.zobrist = hash_position(&non_capturable_without_ep);

        let mut capturable_without_ep = capturable.clone();
        capturable_without_ep.ep_square = None;
        capturable_without_ep.zobrist = hash_position(&capturable_without_ep);

        assert_eq!(effective_ep_file(&non_capturable), None);
        assert_eq!(non_capturable.zobrist, non_capturable_without_ep.zobrist);
        assert_eq!(effective_ep_file(&capturable), Some(4));
        assert_ne!(capturable.zobrist, capturable_without_ep.zobrist);
    }
}
