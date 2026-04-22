#![allow(dead_code)]

use crate::core::position::Position;
use crate::core::types::{Color, PieceKind, Square};

pub type Score = i32;

const MATERIAL: [Score; 6] = [100, 320, 330, 500, 900, 0];

const PAWN_PST: [Score; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, //
    2, 4, 6, 8, 8, 6, 4, 2, //
    4, 6, 10, 14, 14, 10, 6, 4, //
    6, 8, 14, 20, 20, 14, 8, 6, //
    8, 10, 18, 26, 26, 18, 10, 8, //
    12, 14, 22, 30, 30, 22, 14, 12, //
    18, 20, 28, 36, 36, 28, 20, 18, //
    0, 0, 0, 0, 0, 0, 0, 0,
];

const KNIGHT_PST: [Score; 64] = [
    -20, -12, -8, -6, -6, -8, -12, -20, //
    -12, -4, 2, 4, 4, 2, -4, -12, //
    -8, 2, 8, 12, 12, 8, 2, -8, //
    -6, 4, 12, 18, 18, 12, 4, -6, //
    -6, 4, 12, 18, 18, 12, 4, -6, //
    -8, 2, 8, 12, 12, 8, 2, -8, //
    -12, -4, 2, 4, 4, 2, -4, -12, //
    -20, -12, -8, -6, -6, -8, -12, -20,
];

const BISHOP_PST: [Score; 64] = [
    -10, -6, -4, -2, -2, -4, -6, -10, //
    -6, 2, 4, 6, 6, 4, 2, -6, //
    -4, 4, 8, 10, 10, 8, 4, -4, //
    -2, 6, 10, 14, 14, 10, 6, -2, //
    -2, 6, 10, 14, 14, 10, 6, -2, //
    -4, 4, 8, 10, 10, 8, 4, -4, //
    -6, 2, 4, 6, 6, 4, 2, -6, //
    -10, -6, -4, -2, -2, -4, -6, -10,
];

const ROOK_PST: [Score; 64] = [
    0, 0, 2, 4, 4, 2, 0, 0, //
    2, 2, 4, 6, 6, 4, 2, 2, //
    4, 4, 6, 8, 8, 6, 4, 4, //
    6, 6, 8, 10, 10, 8, 6, 6, //
    8, 8, 10, 12, 12, 10, 8, 8, //
    10, 10, 12, 14, 14, 12, 10, 10, //
    12, 12, 14, 16, 16, 14, 12, 12, //
    14, 14, 16, 18, 18, 16, 14, 14,
];

const QUEEN_PST: [Score; 64] = [
    -8, -4, -2, 0, 0, -2, -4, -8, //
    -4, 2, 4, 6, 6, 4, 2, -4, //
    -2, 4, 8, 10, 10, 8, 4, -2, //
    0, 6, 10, 14, 14, 10, 6, 0, //
    0, 6, 10, 14, 14, 10, 6, 0, //
    -2, 4, 8, 10, 10, 8, 4, -2, //
    -4, 2, 4, 6, 6, 4, 2, -4, //
    -8, -4, -2, 0, 0, -2, -4, -8,
];

const KING_PST: [Score; 64] = [0; 64];

pub fn evaluate(pos: &Position) -> Score {
    let mut white_minus_black = 0;

    for square_idx in 0..64 {
        let Some(piece) = pos.board[square_idx] else {
            continue;
        };

        let square = Square(square_idx as u8);
        let piece_score =
            material_value(piece.kind()) + pst_value(piece.kind(), square, piece.color());

        match piece.color() {
            Color::White => white_minus_black += piece_score,
            Color::Black => white_minus_black -= piece_score,
        }
    }

    match pos.side_to_move {
        Color::White => white_minus_black,
        Color::Black => -white_minus_black,
    }
}

pub(crate) const fn material_value(kind: PieceKind) -> Score {
    MATERIAL[kind.idx()]
}

fn pst_value(kind: PieceKind, square: Square, color: Color) -> Score {
    let idx = match color {
        Color::White => square.idx(),
        Color::Black => mirror_square(square).idx(),
    };

    match kind {
        PieceKind::Pawn => PAWN_PST[idx],
        PieceKind::Knight => KNIGHT_PST[idx],
        PieceKind::Bishop => BISHOP_PST[idx],
        PieceKind::Rook => ROOK_PST[idx],
        PieceKind::Queen => QUEEN_PST[idx],
        PieceKind::King => KING_PST[idx],
    }
}

const fn mirror_square(square: Square) -> Square {
    Square::from_file_rank(square.file(), 7 - square.rank())
}

#[cfg(test)]
mod test {
    use super::*;

    const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn start_position_evaluates_to_zero() {
        let pos = Position::from_fen(STARTPOS).unwrap();

        assert_eq!(evaluate(&pos), 0);
    }

    #[test]
    fn extra_material_uses_side_to_move_perspective() {
        let white_to_move = Position::from_fen("4k3/8/8/8/8/8/8/4KQ2 w - - 0 1").unwrap();
        let black_to_move = Position::from_fen("4k3/8/8/8/8/8/8/4KQ2 b - - 0 1").unwrap();

        assert!(evaluate(&white_to_move) > 0);
        assert!(evaluate(&black_to_move) < 0);
        assert_eq!(evaluate(&white_to_move), -evaluate(&black_to_move));
    }

    #[test]
    fn mirrored_piece_square_scores_are_equal_and_opposite() {
        let white_piece = Position::from_fen("4k3/8/8/8/8/2N5/8/4K3 w - - 0 1").unwrap();
        let black_piece = Position::from_fen("4k3/8/2n5/8/8/8/8/4K3 w - - 0 1").unwrap();

        assert_eq!(evaluate(&white_piece), -evaluate(&black_piece));
    }
}
