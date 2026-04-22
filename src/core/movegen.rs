#![allow(dead_code)]

use crate::core::mv::Move;
use crate::core::position::{CastlingRights, Position};
use crate::core::types::{Color, Piece, PieceKind, Square};

const KNIGHT_OFFSETS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];

const KING_OFFSETS: [(i8, i8); 8] = [
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, 1),
];

const BISHOP_DIRECTIONS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, -1), (-1, 1)];
const ROOK_DIRECTIONS: [(i8, i8); 4] = [(1, 0), (0, -1), (-1, 0), (0, 1)];
const PROMOTION_PIECES: [PieceKind; 4] = [
    PieceKind::Knight,
    PieceKind::Bishop,
    PieceKind::Rook,
    PieceKind::Queen,
];

pub fn is_square_attacked(pos: &Position, square: Square, by: Color) -> bool {
    is_attacked_by_pawn(pos, square, by)
        || is_attacked_by_leaper(pos, square, by, PieceKind::Knight, &KNIGHT_OFFSETS)
        || is_attacked_by_leaper(pos, square, by, PieceKind::King, &KING_OFFSETS)
        || is_attacked_by_slider(
            pos,
            square,
            by,
            &BISHOP_DIRECTIONS,
            PieceKind::Bishop,
            PieceKind::Queen,
        )
        || is_attacked_by_slider(
            pos,
            square,
            by,
            &ROOK_DIRECTIONS,
            PieceKind::Rook,
            PieceKind::Queen,
        )
}

pub fn in_check(pos: &Position, color: Color) -> bool {
    is_square_attacked(pos, pos.king_sq[color.idx()], color.opposit())
}

pub fn generate_pseudo_legal(pos: &Position, moves: &mut Vec<Move>) {
    moves.clear();

    let color = pos.side_to_move;
    generate_pawn_moves(pos, color, moves);
    generate_leaper_moves(pos, color, PieceKind::Knight, &KNIGHT_OFFSETS, moves);
    generate_slider_moves(pos, color, PieceKind::Bishop, &BISHOP_DIRECTIONS, moves);
    generate_slider_moves(pos, color, PieceKind::Rook, &ROOK_DIRECTIONS, moves);
    generate_slider_moves(pos, color, PieceKind::Queen, &BISHOP_DIRECTIONS, moves);
    generate_slider_moves(pos, color, PieceKind::Queen, &ROOK_DIRECTIONS, moves);
    generate_leaper_moves(pos, color, PieceKind::King, &KING_OFFSETS, moves);
    generate_castling_moves(pos, color, moves);
}

pub fn generate_legal(pos: &mut Position, moves: &mut Vec<Move>) {
    moves.clear();
    let moving_side = pos.side_to_move;
    let mut pseudo = Vec::new();
    generate_pseudo_legal(pos, &mut pseudo);

    for mv in pseudo {
        let state = pos.make_move(mv);
        let is_legal =
            !is_square_attacked(pos, pos.king_sq[moving_side.idx()], moving_side.opposit());
        pos.unmake_move(mv, state);

        if is_legal {
            moves.push(mv);
        }
    }
}

pub fn perft(pos: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut moves = Vec::new();
    generate_legal(pos, &mut moves);
    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0;
    for mv in moves {
        let state = pos.make_move(mv);
        nodes += perft(pos, depth - 1);
        pos.unmake_move(mv, state);
    }
    nodes
}

fn is_attacked_by_pawn(pos: &Position, square: Square, by: Color) -> bool {
    let attacker_rank_delta = match by {
        Color::White => -1,
        Color::Black => 1,
    };

    for file_delta in [-1, 1] {
        if let Some(from) = offset_square(square, file_delta, attacker_rank_delta)
            && pos.piece_at(from) == Some(Piece::from(by, PieceKind::Pawn))
        {
            return true;
        }
    }

    false
}

fn is_attacked_by_leaper(
    pos: &Position,
    square: Square,
    by: Color,
    kind: PieceKind,
    offsets: &[(i8, i8)],
) -> bool {
    let attacker = Piece::from(by, kind);
    offsets.iter().any(|&(file_delta, rank_delta)| {
        offset_square(square, file_delta, rank_delta)
            .is_some_and(|from| pos.piece_at(from) == Some(attacker))
    })
}

fn is_attacked_by_slider(
    pos: &Position,
    square: Square,
    by: Color,
    directions: &[(i8, i8)],
    primary: PieceKind,
    secondary: PieceKind,
) -> bool {
    for &(file_delta, rank_delta) in directions {
        let mut current = square;
        while let Some(next) = offset_square(current, file_delta, rank_delta) {
            current = next;
            let Some(piece) = pos.piece_at(current) else {
                continue;
            };

            if piece.color() == by && (piece.kind() == primary || piece.kind() == secondary) {
                return true;
            }
            break;
        }
    }

    false
}

fn generate_pawn_moves(pos: &Position, color: Color, moves: &mut Vec<Move>) {
    let direction = pawn_push_direction(color);
    let start_rank = pawn_start_rank(color);
    let promotion_rank = pawn_promotion_rank(color);
    let mut pawns = pos.pieces_of(color, PieceKind::Pawn);

    while let Some(from) = pawns.pop_lsb() {
        if let Some(one_step) = offset_square(from, 0, direction)
            && pos.piece_at(one_step).is_none()
        {
            if one_step.rank() == promotion_rank {
                push_promotions(from, one_step, false, moves);
            } else {
                moves.push(Move::new(from, one_step));
            }

            if from.rank() == start_rank
                && let Some(two_step) = offset_square(one_step, 0, direction)
                && pos.piece_at(two_step).is_none()
            {
                moves.push(Move::new(from, two_step).with_double_pawn_push());
            }
        }

        for file_delta in [-1, 1] {
            if let Some(to) = offset_square(from, file_delta, direction) {
                if pos
                    .piece_at(to)
                    .is_some_and(|piece| piece.color() == color.opposit())
                {
                    if to.rank() == promotion_rank {
                        push_promotions(from, to, true, moves);
                    } else {
                        moves.push(Move::new(from, to).with_capture());
                    }
                } else if pos.ep_square == Some(to) {
                    moves.push(Move::new(from, to).with_en_passant());
                }
            }
        }
    }
}

fn generate_leaper_moves(
    pos: &Position,
    color: Color,
    kind: PieceKind,
    offsets: &[(i8, i8)],
    moves: &mut Vec<Move>,
) {
    let mut pieces = pos.pieces_of(color, kind);

    while let Some(from) = pieces.pop_lsb() {
        for &(file_delta, rank_delta) in offsets {
            let Some(to) = offset_square(from, file_delta, rank_delta) else {
                continue;
            };
            push_if_valid_destination(pos, color, from, to, moves);
        }
    }
}

fn generate_slider_moves(
    pos: &Position,
    color: Color,
    kind: PieceKind,
    directions: &[(i8, i8)],
    moves: &mut Vec<Move>,
) {
    let mut pieces = pos.pieces_of(color, kind);

    while let Some(from) = pieces.pop_lsb() {
        for &(file_delta, rank_delta) in directions {
            let mut current = from;
            while let Some(to) = offset_square(current, file_delta, rank_delta) {
                current = to;
                if let Some(piece) = pos.piece_at(to) {
                    if piece.color() != color {
                        moves.push(Move::new(from, to).with_capture());
                    }
                    break;
                }

                moves.push(Move::new(from, to));
            }
        }
    }
}

fn push_promotions(from: Square, to: Square, capture: bool, moves: &mut Vec<Move>) {
    for promoted in PROMOTION_PIECES {
        let mv = Move::with_promotion(from, to, promoted);
        moves.push(if capture { mv.with_capture() } else { mv });
    }
}

fn generate_castling_moves(pos: &Position, color: Color, moves: &mut Vec<Move>) {
    let rank = match color {
        Color::White => 0,
        Color::Black => 7,
    };
    let king_from = Square::from_file_rank(4, rank);
    let kingside_rook = Square::from_file_rank(7, rank);
    let queenside_rook = Square::from_file_rank(0, rank);
    let king = Piece::from(color, PieceKind::King);
    let rook = Piece::from(color, PieceKind::Rook);

    if pos.piece_at(king_from) != Some(king) {
        return;
    }

    let opponent = color.opposit();
    if is_square_attacked(pos, king_from, opponent) {
        return;
    }

    let (kingside_right, queenside_right) = match color {
        Color::White => (
            CastlingRights::WHITE_KINGSIDE,
            CastlingRights::WHITE_QUEENSIDE,
        ),
        Color::Black => (
            CastlingRights::BLACK_KINGSIDE,
            CastlingRights::BLACK_QUEENSIDE,
        ),
    };

    if pos.castling.has(kingside_right)
        && pos.piece_at(kingside_rook) == Some(rook)
        && pos.piece_at(Square::from_file_rank(5, rank)).is_none()
        && pos.piece_at(Square::from_file_rank(6, rank)).is_none()
        && !is_square_attacked(pos, Square::from_file_rank(5, rank), opponent)
        && !is_square_attacked(pos, Square::from_file_rank(6, rank), opponent)
    {
        moves.push(Move::new(king_from, Square::from_file_rank(6, rank)).with_castling());
    }

    if pos.castling.has(queenside_right)
        && pos.piece_at(queenside_rook) == Some(rook)
        && pos.piece_at(Square::from_file_rank(1, rank)).is_none()
        && pos.piece_at(Square::from_file_rank(2, rank)).is_none()
        && pos.piece_at(Square::from_file_rank(3, rank)).is_none()
        && !is_square_attacked(pos, Square::from_file_rank(2, rank), opponent)
        && !is_square_attacked(pos, Square::from_file_rank(3, rank), opponent)
    {
        moves.push(Move::new(king_from, Square::from_file_rank(2, rank)).with_castling());
    }
}

fn push_if_valid_destination(
    pos: &Position,
    color: Color,
    from: Square,
    to: Square,
    moves: &mut Vec<Move>,
) {
    match pos.piece_at(to) {
        Some(piece) if piece.color() == color => {}
        Some(_) => moves.push(Move::new(from, to).with_capture()),
        None => moves.push(Move::new(from, to)),
    }
}

const fn pawn_push_direction(color: Color) -> i8 {
    match color {
        Color::White => 1,
        Color::Black => -1,
    }
}

const fn pawn_start_rank(color: Color) -> u8 {
    match color {
        Color::White => 1,
        Color::Black => 6,
    }
}

const fn pawn_promotion_rank(color: Color) -> u8 {
    match color {
        Color::White => 7,
        Color::Black => 0,
    }
}

fn offset_square(square: Square, file_delta: i8, rank_delta: i8) -> Option<Square> {
    let file = square.file() as i8 + file_delta;
    let rank = square.rank() as i8 + rank_delta;

    if (0..8).contains(&file) && (0..8).contains(&rank) {
        Some(Square::from_file_rank(file as u8, rank as u8))
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::zobrist;

    fn parse_pos(fen: &str) -> Position {
        Position::from_fen(fen).unwrap()
    }

    fn assert_hash_consistent(pos: &Position) {
        assert_eq!(pos.zobrist, zobrist::hash_position(pos));
    }

    fn assert_hash_invariants_at_depth(pos: &mut Position, depth: u32) {
        assert_hash_consistent(pos);
        if depth == 0 {
            return;
        }

        let mut moves = Vec::new();
        generate_legal(pos, &mut moves);

        for mv in moves {
            let before = pos.clone();
            let before_hash = pos.zobrist;
            let state = pos.make_move(mv);

            assert_hash_consistent(pos);
            assert_hash_invariants_at_depth(pos, depth - 1);

            pos.unmake_move(mv, state);
            assert_eq!(pos.zobrist, before_hash);
            assert_eq!(pos, &before);
            assert_hash_consistent(pos);
        }
    }

    fn has_move(moves: &[Move], from: Square, to: Square) -> bool {
        moves.iter().any(|mv| mv.from() == from && mv.to() == to)
    }

    fn has_capture(moves: &[Move], from: Square, to: Square) -> bool {
        moves
            .iter()
            .any(|mv| mv.from() == from && mv.to() == to && mv.is_capture())
    }

    fn count_promotions(moves: &[Move], from: Square, to: Square) -> usize {
        moves
            .iter()
            .filter(|mv| mv.from() == from && mv.to() == to && mv.promotion().is_some())
            .count()
    }

    fn has_castling(moves: &[Move], from: Square, to: Square) -> bool {
        moves
            .iter()
            .any(|mv| mv.from() == from && mv.to() == to && mv.is_castling())
    }

    #[test]
    fn detects_pawn_attack_direction_for_both_colors() {
        let pos = parse_pos("4k3/8/8/5p2/3P4/8/8/4K3 w - - 0 1");

        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(4, 4),
            Color::White
        ));
        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(4, 3),
            Color::Black
        ));
        assert!(!is_square_attacked(
            &pos,
            Square::from_file_rank(4, 3),
            Color::White
        ));
    }

    #[test]
    fn detects_knight_attacks_from_center_and_edge() {
        let pos = parse_pos("4k3/8/8/8/3N4/8/8/4K3 w - - 0 1");

        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(4, 5),
            Color::White
        ));
        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(1, 4),
            Color::White
        ));
        assert!(!is_square_attacked(
            &pos,
            Square::from_file_rank(4, 4),
            Color::White
        ));

        let edge = parse_pos("4k3/8/8/8/8/8/8/N3K3 w - - 0 1");
        assert!(is_square_attacked(
            &edge,
            Square::from_file_rank(1, 2),
            Color::White
        ));
        assert!(is_square_attacked(
            &edge,
            Square::from_file_rank(2, 1),
            Color::White
        ));
    }

    #[test]
    fn detects_king_attacks_from_center_and_edge() {
        let pos = parse_pos("4k3/8/8/8/3K4/8/8/8 w - - 0 1");

        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(4, 4),
            Color::White
        ));
        assert!(!is_square_attacked(
            &pos,
            Square::from_file_rank(5, 5),
            Color::White
        ));

        let edge = parse_pos("4k3/8/8/8/8/8/8/K7 w - - 0 1");
        assert!(is_square_attacked(
            &edge,
            Square::from_file_rank(0, 1),
            Color::White
        ));
        assert!(is_square_attacked(
            &edge,
            Square::from_file_rank(1, 0),
            Color::White
        ));
    }

    #[test]
    fn detects_blocked_and_unblocked_bishop_rays() {
        let pos = parse_pos("4k3/8/6B1/8/8/2P5/8/4K3 w - - 0 1");

        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(4, 3),
            Color::White
        ));
        assert!(!is_square_attacked(
            &pos,
            Square::from_file_rank(1, 1),
            Color::White
        ));
    }

    #[test]
    fn detects_blocked_and_unblocked_rook_rays() {
        let pos = parse_pos("4k3/8/8/4R3/8/4P3/8/K7 w - - 0 1");

        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(4, 6),
            Color::White
        ));
        assert!(!is_square_attacked(
            &pos,
            Square::from_file_rank(4, 1),
            Color::White
        ));
    }

    #[test]
    fn queen_combines_rook_and_bishop_rays() {
        let pos = parse_pos("4k3/8/8/3Q4/8/8/8/4K3 w - - 0 1");

        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(3, 7),
            Color::White
        ));
        assert!(is_square_attacked(
            &pos,
            Square::from_file_rank(5, 2),
            Color::White
        ));
    }

    #[test]
    fn in_check_detects_direct_checks_and_ignores_blocked_lines() {
        let checked = parse_pos("4k3/8/8/8/4R3/8/8/4K3 b - - 0 1");
        assert!(in_check(&checked, Color::Black));

        let blocked = parse_pos("4k3/8/4p3/8/4R3/8/8/K7 b - - 0 1");
        assert!(!in_check(&blocked, Color::Black));
    }

    #[test]
    fn start_position_generates_twenty_pseudo_legal_moves() {
        let pos = parse_pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert_eq!(moves.len(), 20);
    }

    #[test]
    fn blocked_starting_pieces_do_not_move_through_own_pieces() {
        let pos = parse_pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(
            !moves
                .iter()
                .any(|mv| mv.from() == Square::from_file_rank(0, 0))
        );
        assert!(
            !moves
                .iter()
                .any(|mv| mv.from() == Square::from_file_rank(2, 0))
        );
        assert!(
            !moves
                .iter()
                .any(|mv| mv.from() == Square::from_file_rank(3, 0))
        );
        assert!(
            !moves
                .iter()
                .any(|mv| mv.from() == Square::from_file_rank(4, 0))
        );
    }

    #[test]
    fn captures_are_generated_against_enemy_pieces_only() {
        let pos = parse_pos("4k3/8/8/5P2/3N4/8/2p1p3/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(has_capture(
            &moves,
            Square::from_file_rank(3, 3),
            Square::from_file_rank(2, 1)
        ));
        assert!(has_capture(
            &moves,
            Square::from_file_rank(3, 3),
            Square::from_file_rank(4, 1)
        ));
        assert!(!has_move(
            &moves,
            Square::from_file_rank(3, 3),
            Square::from_file_rank(5, 4)
        ));
    }

    #[test]
    fn pawns_cannot_push_into_occupied_squares() {
        let pos = parse_pos("4k3/8/8/8/8/4p3/4P3/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(!has_move(
            &moves,
            Square::from_file_rank(4, 1),
            Square::from_file_rank(4, 2)
        ));
        assert!(!has_move(
            &moves,
            Square::from_file_rank(4, 1),
            Square::from_file_rank(4, 3)
        ));
    }

    #[test]
    fn double_pawn_push_requires_both_path_squares_to_be_empty() {
        let clear = parse_pos("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1");
        let blocked = parse_pos("4k3/8/8/8/4p3/8/4P3/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&clear, &mut moves);
        assert!(moves.iter().any(|mv| {
            mv.from() == Square::from_file_rank(4, 1)
                && mv.to() == Square::from_file_rank(4, 3)
                && mv.is_double_pawn_push()
        }));

        generate_pseudo_legal(&blocked, &mut moves);
        assert!(!has_move(
            &moves,
            Square::from_file_rank(4, 1),
            Square::from_file_rank(4, 3)
        ));
    }

    #[test]
    fn quiet_promotions_generate_four_moves() {
        let pos = parse_pos("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert_eq!(
            count_promotions(
                &moves,
                Square::from_file_rank(0, 6),
                Square::from_file_rank(0, 7)
            ),
            4
        );
    }

    #[test]
    fn capture_promotions_generate_four_moves_per_target() {
        let pos = parse_pos("r1n1k3/1P6/8/8/8/8/8/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert_eq!(
            moves
                .iter()
                .filter(|mv| mv.from() == Square::from_file_rank(1, 6)
                    && mv.to() == Square::from_file_rank(0, 7)
                    && mv.promotion().is_some()
                    && mv.is_capture())
                .count(),
            4
        );
        assert_eq!(
            moves
                .iter()
                .filter(|mv| mv.from() == Square::from_file_rank(1, 6)
                    && mv.to() == Square::from_file_rank(2, 7)
                    && mv.promotion().is_some()
                    && mv.is_capture())
                .count(),
            4
        );
    }

    #[test]
    fn promotion_rank_push_does_not_generate_non_promotion_move() {
        let pos = parse_pos("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(!moves.iter().any(|mv| {
            mv.from() == Square::from_file_rank(0, 6)
                && mv.to() == Square::from_file_rank(0, 7)
                && mv.promotion().is_none()
        }));
    }

    #[test]
    fn valid_en_passant_candidate_is_generated() {
        let pos = parse_pos("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(moves.iter().any(|mv| {
            mv.from() == Square::from_file_rank(4, 4)
                && mv.to() == Square::from_file_rank(3, 5)
                && mv.is_en_passant()
                && mv.is_capture()
        }));
    }

    #[test]
    fn en_passant_is_not_generated_for_wrong_pawn() {
        let pos = parse_pos("4k3/8/8/3p2P1/8/8/8/4K3 w - d6 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(!moves.iter().any(|mv| mv.is_en_passant()));
    }

    #[test]
    fn both_white_castling_sides_can_be_generated() {
        let pos = parse_pos("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(6, 0)
        ));
        assert!(has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(2, 0)
        ));
    }

    #[test]
    fn both_black_castling_sides_can_be_generated() {
        let pos = parse_pos("r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(has_castling(
            &moves,
            Square::from_file_rank(4, 7),
            Square::from_file_rank(6, 7)
        ));
        assert!(has_castling(
            &moves,
            Square::from_file_rank(4, 7),
            Square::from_file_rank(2, 7)
        ));
    }

    #[test]
    fn blocked_path_prevents_castling() {
        let pos = parse_pos("4k3/8/8/8/8/8/8/R3KB1R w KQ - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(!has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(6, 0)
        ));
        assert!(has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(2, 0)
        ));
    }

    #[test]
    fn missing_rook_prevents_castling() {
        let pos = parse_pos("4k3/8/8/8/8/8/8/4K2R w Q - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(!has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(2, 0)
        ));
    }

    #[test]
    fn attacked_origin_transit_or_destination_prevents_castling() {
        let mut moves = Vec::new();

        let origin_attacked = parse_pos("k3r3/8/8/8/8/8/8/R3K2R w KQ - 0 1");
        generate_pseudo_legal(&origin_attacked, &mut moves);
        assert!(!moves.iter().any(|mv| mv.is_castling()));

        let transit_attacked = parse_pos("4k3/8/8/8/2b5/8/8/R3K2R w KQ - 0 1");
        generate_pseudo_legal(&transit_attacked, &mut moves);
        assert!(!has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(6, 0)
        ));

        let destination_attacked = parse_pos("2r1k3/8/8/8/8/8/8/R3K2R w KQ - 0 1");
        generate_pseudo_legal(&destination_attacked, &mut moves);
        assert!(!has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(2, 0)
        ));
    }

    #[test]
    fn attacked_rook_square_does_not_prevent_castling() {
        let pos = parse_pos("4k2r/8/8/8/8/8/8/R3K2R w KQ - 0 1");
        let mut moves = Vec::new();

        generate_pseudo_legal(&pos, &mut moves);

        assert!(has_castling(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(6, 0)
        ));
    }

    #[test]
    fn pinned_pieces_cannot_expose_king() {
        let mut pos = parse_pos("k3r3/8/8/8/8/8/4R3/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_legal(&mut pos, &mut moves);

        assert!(!has_move(
            &moves,
            Square::from_file_rank(4, 1),
            Square::from_file_rank(3, 1)
        ));
    }

    #[test]
    fn king_cannot_move_into_check() {
        let mut pos = parse_pos("k3r3/8/8/8/8/8/8/4K3 w - - 0 1");
        let mut moves = Vec::new();

        generate_legal(&mut pos, &mut moves);

        assert!(!has_move(
            &moves,
            Square::from_file_rank(4, 0),
            Square::from_file_rank(4, 1)
        ));
    }

    #[test]
    fn side_in_check_must_respond_legally() {
        let mut pos = parse_pos("k3r3/8/8/8/8/8/8/4K1N1 w - - 0 1");
        let mut moves = Vec::new();

        generate_legal(&mut pos, &mut moves);

        assert!(has_move(
            &moves,
            Square::from_file_rank(6, 0),
            Square::from_file_rank(4, 1)
        ));
        assert!(!has_move(
            &moves,
            Square::from_file_rank(6, 0),
            Square::from_file_rank(7, 2)
        ));
    }

    #[test]
    fn double_check_only_allows_king_moves() {
        let mut pos = parse_pos("k3r3/8/8/8/1b6/8/8/4K1N1 w - - 0 1");
        let mut moves = Vec::new();

        generate_legal(&mut pos, &mut moves);

        assert!(
            moves
                .iter()
                .all(|mv| mv.from() == Square::from_file_rank(4, 0))
        );
    }

    #[test]
    fn legal_generation_leaves_position_unchanged() {
        let mut pos = parse_pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let before = pos.clone();
        let mut moves = Vec::new();

        generate_legal(&mut pos, &mut moves);

        assert_eq!(pos, before);
    }

    #[test]
    fn start_position_has_twenty_legal_moves() {
        let mut pos = parse_pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let mut moves = Vec::new();

        generate_legal(&mut pos, &mut moves);

        assert_eq!(moves.len(), 20);
    }

    #[test]
    fn perft_start_position_matches_known_counts() {
        let expected = [(1, 20), (2, 400), (3, 8902), (4, 197_281)];

        for (depth, nodes) in expected {
            let mut pos = parse_pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
            let before = pos.clone();

            assert_eq!(perft(&mut pos, depth), nodes);
            assert_eq!(pos, before);
        }
    }

    #[test]
    fn perft_kiwipete_matches_known_counts() {
        let expected = [(1, 48), (2, 2039), (3, 97_862)];

        for (depth, nodes) in expected {
            let mut pos =
                parse_pos("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1");
            let before = pos.clone();

            assert_eq!(perft(&mut pos, depth), nodes);
            assert_eq!(pos, before);
        }
    }

    #[test]
    fn perft_covers_shallow_special_move_position() {
        let mut pos = parse_pos("4k3/P7/8/3pP3/8/8/8/R3K2R w KQ d6 0 1");
        let before = pos.clone();

        assert_eq!(perft(&mut pos, 1), 30);
        assert_eq!(pos, before);
    }

    // Position 3 from https://www.chessprogramming.org/Perft_Results
    #[test]
    fn perft_pos3_matches_known_counts() {
        let expected = [(1, 14), (2, 191), (3, 2_812), (4, 43_238), (5, 674_624)];
        for (depth, nodes) in expected {
            let mut pos = parse_pos("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1");
            let before = pos.clone();
            assert_eq!(perft(&mut pos, depth), nodes);
            assert_eq!(pos, before);
        }
    }

    #[test]
    fn zobrist_hash_invariants_hold_through_shallow_move_trees() {
        let cases = [
            (
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                3,
            ),
            (
                "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
                2,
            ),
            ("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", 2),
            ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", 2),
            ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", 2),
        ];

        for (fen, depth) in cases {
            let mut pos = parse_pos(fen);
            assert_hash_invariants_at_depth(&mut pos, depth);
        }
    }
}
