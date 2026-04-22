#![allow(dead_code)]

use crate::core::eval::{Score, evaluate, material_value};
use crate::core::movegen::{generate_legal, in_check};
use crate::core::mv::Move;
use crate::core::position::{Position, State};
use crate::core::types::{PieceKind, Square};
use std::time::Instant;

pub const MATE_SCORE: Score = 30_000;
pub const INF: Score = 32_000;
const DEFAULT_TT_ENTRIES: usize = 1 << 20;
const TT_MATE_THRESHOLD: Score = MATE_SCORE - 1_000;

#[derive(Debug, Clone)]
struct TranspositionTable {
    entries: Vec<Option<TtEntry>>,
}

impl TranspositionTable {
    fn new(entry_count: usize) -> Self {
        Self {
            entries: vec![None; entry_count.max(1)],
        }
    }

    fn probe(&self, key: u64) -> Option<TtEntry> {
        let entry = self.entries[self.index(key)]?;
        (entry.key == key).then_some(entry)
    }

    fn store(&mut self, entry: TtEntry) {
        let index = self.index(entry.key);
        if self.entries[index].is_none_or(|old| old.key == entry.key || entry.depth >= old.depth) {
            self.entries[index] = Some(entry);
        }
    }

    fn index(&self, key: u64) -> usize {
        key as usize % self.entries.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TtEntry {
    key: u64,
    depth: u8,
    score: Score,
    bound: TtBound,
    best_move: Option<Move>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtBound {
    Exact,
    Lower,
    Upper,
}

fn score_to_tt(score: Score, ply: Score) -> Score {
    if score > TT_MATE_THRESHOLD {
        score + ply
    } else if score < -TT_MATE_THRESHOLD {
        score - ply
    } else {
        score
    }
}

fn score_from_tt(score: Score, ply: Score) -> Score {
    if score > TT_MATE_THRESHOLD {
        score - ply
    } else if score < -TT_MATE_THRESHOLD {
        score + ply
    } else {
        score
    }
}

fn probe_tt(
    tt: &TranspositionTable,
    key: u64,
    depth: u8,
    alpha: Score,
    beta: Score,
    ply: Score,
) -> Option<Score> {
    let entry = tt.probe(key)?;
    score_from_tt_entry(entry, depth, alpha, beta, ply)
}

fn score_from_tt_entry(
    entry: TtEntry,
    depth: u8,
    alpha: Score,
    beta: Score,
    ply: Score,
) -> Option<Score> {
    if entry.depth < depth {
        return None;
    }

    let score = score_from_tt(entry.score, ply);
    match entry.bound {
        TtBound::Exact => Some(score),
        TtBound::Lower if score >= beta => Some(score),
        TtBound::Upper if score <= alpha => Some(score),
        TtBound::Lower | TtBound::Upper => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchLimits {
    pub depth: u8,
    pub deadline: Option<Instant>,
}

impl SearchLimits {
    pub const fn depth(depth: u8) -> Self {
        Self {
            depth,
            deadline: None,
        }
    }

    pub const fn timed(max_depth: u8, deadline: Instant) -> Self {
        Self {
            depth: max_depth,
            deadline: Some(deadline),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: Score,
    pub nodes: u64,
}

pub fn search(pos: &mut Position, history: &[State], limits: SearchLimits) -> SearchResult {
    let mut tt = TranspositionTable::new(DEFAULT_TT_ENTRIES);

    if limits.deadline.is_some() && limits.depth > 1 {
        return iterative_search(pos, history, limits, &mut tt);
    }

    fixed_depth_search(pos, history, limits.depth, limits.deadline, &mut tt)
        .unwrap_or_else(|| fallback_result(pos))
}

fn iterative_search(
    pos: &mut Position,
    history: &[State],
    limits: SearchLimits,
    tt: &mut TranspositionTable,
) -> SearchResult {
    let mut best = None;
    let mut total_nodes = 0;

    for depth in 1..=limits.depth {
        if deadline_expired(limits.deadline) {
            break;
        }

        let Some(mut result) = fixed_depth_search(pos, history, depth, limits.deadline, tt) else {
            break;
        };

        total_nodes += result.nodes;
        result.nodes = total_nodes;
        let terminal = result.best_move.is_none();
        best = Some(result);

        if terminal {
            break;
        }
    }

    best.unwrap_or_else(|| fallback_result(pos))
}

fn fixed_depth_search(
    pos: &mut Position,
    history: &[State],
    depth: u8,
    deadline: Option<Instant>,
    tt: &mut TranspositionTable,
) -> Option<SearchResult> {
    if deadline_expired(deadline) {
        return None;
    }

    let mut nodes = 1;
    let mut history = history.to_vec();

    if depth == 0 {
        let score = quiescence(pos, &mut history, -INF, INF, 0, &mut nodes, deadline)?;
        return Some(SearchResult {
            best_move: None,
            score,
            nodes,
        });
    }
    if is_draw(pos, &history) {
        return Some(SearchResult {
            best_move: None,
            score: 0,
            nodes,
        });
    }

    let mut moves = Vec::new();
    generate_legal(pos, &mut moves);

    if moves.is_empty() {
        return Some(SearchResult {
            best_move: None,
            score: terminal_score(pos, 0),
            nodes,
        });
    }

    order_moves(&mut moves);

    let mut best_move = None;
    let mut best_score = -INF;
    let mut alpha = -INF;

    for mv in moves {
        let state = pos.make_move(mv);
        history.push(state);
        let move_score = -negamax(
            pos,
            &mut history,
            depth.saturating_sub(1),
            -INF,
            -alpha,
            1,
            &mut nodes,
            deadline,
            tt,
        )?;
        history.pop();
        pos.unmake_move(mv, state);

        if move_score > best_score {
            best_score = move_score;
            best_move = Some(mv);
        }
        alpha = alpha.max(best_score);
    }

    Some(SearchResult {
        best_move,
        score: best_score,
        nodes,
    })
}

fn negamax(
    pos: &mut Position,
    history: &mut Vec<State>,
    depth: u8,
    mut alpha: Score,
    beta: Score,
    ply: Score,
    nodes: &mut u64,
    deadline: Option<Instant>,
    tt: &mut TranspositionTable,
) -> Option<Score> {
    if deadline_expired(deadline) {
        return None;
    }

    *nodes += 1;

    if is_draw(pos, history) {
        return Some(0);
    }

    let key = pos.zobrist;
    let tt_entry = tt.probe(key);
    if let Some(score) =
        tt_entry.and_then(|entry| score_from_tt_entry(entry, depth, alpha, beta, ply))
    {
        return Some(score);
    }

    if depth == 0 {
        return quiescence(pos, history, alpha, beta, ply, nodes, deadline);
    }

    let original_alpha = alpha;

    let mut moves = Vec::new();
    generate_legal(pos, &mut moves);

    if moves.is_empty() {
        let score = terminal_score(pos, ply);
        tt.store(TtEntry {
            key,
            depth,
            score: score_to_tt(score, ply),
            bound: TtBound::Exact,
            best_move: None,
        });
        return Some(score);
    }

    order_moves_with_hash_move(&mut moves, tt_entry.and_then(|entry| entry.best_move));

    let mut best_score = -INF;
    let mut best_move = None;
    for mv in moves {
        let state = pos.make_move(mv);
        history.push(state);
        let Some(score) = negamax(
            pos,
            history,
            depth - 1,
            -beta,
            -alpha,
            ply + 1,
            nodes,
            deadline,
            tt,
        ) else {
            history.pop();
            pos.unmake_move(mv, state);
            return None;
        };
        let score = -score;
        history.pop();
        pos.unmake_move(mv, state);

        if score > best_score {
            best_score = score;
            best_move = Some(mv);
        }
        alpha = alpha.max(best_score);
        if alpha >= beta {
            break;
        }
    }

    let bound = if best_score <= original_alpha {
        TtBound::Upper
    } else if best_score >= beta {
        TtBound::Lower
    } else {
        TtBound::Exact
    };
    tt.store(TtEntry {
        key,
        depth,
        score: score_to_tt(best_score, ply),
        bound,
        best_move,
    });

    Some(best_score)
}

fn fallback_result(pos: &mut Position) -> SearchResult {
    let mut moves = Vec::new();
    generate_legal(pos, &mut moves);
    order_moves(&mut moves);

    if let Some(best_move) = moves.first().copied() {
        SearchResult {
            best_move: Some(best_move),
            score: evaluate(pos),
            nodes: 1,
        }
    } else {
        SearchResult {
            best_move: None,
            score: terminal_score(pos, 0),
            nodes: 1,
        }
    }
}

fn deadline_expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn is_draw(pos: &Position, history: &[State]) -> bool {
    pos.halfmove_clock >= 100 || pos.is_threefold_repetition(history)
}

fn terminal_score(pos: &Position, ply: Score) -> Score {
    if in_check(pos, pos.side_to_move) {
        -MATE_SCORE + ply
    } else {
        0
    }
}

fn quiescence(
    pos: &mut Position,
    history: &mut Vec<State>,
    mut alpha: Score,
    beta: Score,
    ply: Score,
    nodes: &mut u64,
    deadline: Option<Instant>,
) -> Option<Score> {
    if deadline_expired(deadline) {
        return None;
    }

    *nodes += 1;

    if is_draw(pos, history) {
        return Some(0);
    }

    let checked = in_check(pos, pos.side_to_move);
    let mut moves = Vec::new();
    generate_legal(pos, &mut moves);

    if moves.is_empty() {
        return Some(terminal_score(pos, ply));
    }

    if !checked {
        let stand_pat = evaluate(pos);
        if stand_pat >= beta {
            return Some(beta);
        }
        alpha = alpha.max(stand_pat);
        moves.retain(|mv| is_quiescence_move(*mv));
    }

    order_quiescence_moves(pos, &mut moves);

    for mv in moves {
        let state = pos.make_move(mv);
        history.push(state);
        let Some(score) = quiescence(pos, history, -beta, -alpha, ply + 1, nodes, deadline) else {
            history.pop();
            pos.unmake_move(mv, state);
            return None;
        };
        let score = -score;
        history.pop();
        pos.unmake_move(mv, state);

        alpha = alpha.max(score);
        if alpha >= beta {
            return Some(beta);
        }
    }

    Some(alpha)
}

fn is_quiescence_move(mv: Move) -> bool {
    mv.is_capture() || mv.promotion().is_some()
}

fn order_moves(moves: &mut [Move]) {
    order_moves_with_hash_move(moves, None);
}

fn order_moves_with_hash_move(moves: &mut [Move], hash_move: Option<Move>) {
    moves.sort_by_key(|mv| {
        let hash_score = if Some(*mv) == hash_move { 1 } else { 0 };
        (-hash_score, -move_order_score(*mv))
    });
}

fn order_quiescence_moves(pos: &Position, moves: &mut [Move]) {
    moves.sort_by_key(|mv| -quiescence_move_order_score(pos, *mv));
}

fn quiescence_move_order_score(pos: &Position, mv: Move) -> Score {
    let capture_score = match (capture_victim(pos, mv), capture_attacker(pos, mv)) {
        (Some(victim), Some(attacker)) => {
            10_000 + material_value_for_ordering(victim) * 10
                - material_value_for_ordering(attacker)
        }
        _ if mv.is_capture() => 10_000,
        _ => 0,
    };
    let promotion_score = mv.promotion().map_or(0, material_value_for_ordering);

    capture_score + promotion_score
}

fn capture_victim(pos: &Position, mv: Move) -> Option<PieceKind> {
    if mv.is_capture() {
        pos.piece_at(capture_square(mv)).map(|piece| piece.kind())
    } else {
        None
    }
}

fn capture_square(mv: Move) -> Square {
    if mv.is_en_passant() {
        Square::from_file_rank(mv.to().file(), mv.from().rank())
    } else {
        mv.to()
    }
}

fn capture_attacker(pos: &Position, mv: Move) -> Option<PieceKind> {
    if mv.is_capture() {
        pos.piece_at(mv.from()).map(|piece| piece.kind())
    } else {
        None
    }
}

fn move_order_score(mv: Move) -> Score {
    let capture_score = if mv.is_capture() { 10_000 } else { 0 };
    let promotion_score = mv.promotion().map_or(0, material_value_for_ordering);

    capture_score + promotion_score
}

fn material_value_for_ordering(kind: PieceKind) -> Score {
    material_value(kind)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::movegen::generate_legal;
    use crate::core::types::Square;
    use std::time::Duration;

    fn parse_pos(fen: &str) -> Position {
        Position::from_fen(fen).unwrap()
    }

    fn assert_search_preserves_position(fen: &str, depth: u8) {
        let mut pos = parse_pos(fen);
        let before = pos.clone();

        search(&mut pos, &[], SearchLimits::depth(depth));

        assert_eq!(pos, before);
    }

    fn has_no_legal_moves(pos: &mut Position) -> bool {
        let mut moves = Vec::new();
        generate_legal(pos, &mut moves);
        moves.is_empty()
    }

    fn tt_entry(key: u64, depth: u8, score: Score) -> TtEntry {
        tt_entry_with_bound(key, depth, score, TtBound::Exact)
    }

    fn tt_entry_with_bound(key: u64, depth: u8, score: Score, bound: TtBound) -> TtEntry {
        TtEntry {
            key,
            depth,
            score,
            bound,
            best_move: None,
        }
    }

    #[test]
    fn transposition_table_returns_matching_key() {
        let mut table = TranspositionTable::new(16);
        let entry = tt_entry(42, 3, 100);

        table.store(entry);

        assert_eq!(table.probe(42), Some(entry));
    }

    #[test]
    fn transposition_table_ignores_colliding_non_matching_key() {
        let mut table = TranspositionTable::new(4);

        table.store(tt_entry(1, 3, 100));

        assert_eq!(table.probe(5), None);
    }

    #[test]
    fn transposition_table_replaces_same_key() {
        let mut table = TranspositionTable::new(4);
        let replacement = tt_entry(1, 1, 200);

        table.store(tt_entry(1, 5, 100));
        table.store(replacement);

        assert_eq!(table.probe(1), Some(replacement));
    }

    #[test]
    fn transposition_table_replaces_shallower_collision_with_deeper_entry() {
        let mut table = TranspositionTable::new(4);
        let deeper = tt_entry(5, 4, 200);

        table.store(tt_entry(1, 2, 100));
        table.store(deeper);

        assert_eq!(table.probe(1), None);
        assert_eq!(table.probe(5), Some(deeper));
    }

    #[test]
    fn transposition_table_keeps_deeper_entry_over_shallower_collision() {
        let mut table = TranspositionTable::new(4);
        let deeper = tt_entry(1, 4, 100);

        table.store(deeper);
        table.store(tt_entry(5, 2, 200));

        assert_eq!(table.probe(1), Some(deeper));
        assert_eq!(table.probe(5), None);
    }

    #[test]
    fn tt_score_normalization_leaves_ordinary_scores_unchanged() {
        for score in [-500, 0, 500, TT_MATE_THRESHOLD] {
            let stored = score_to_tt(score, 7);

            assert_eq!(stored, score);
            assert_eq!(score_from_tt(stored, 3), score);
        }
    }

    #[test]
    fn tt_score_normalization_round_trips_winning_mates_across_plies() {
        let root_relative_score = MATE_SCORE - 5;
        let stored = score_to_tt(root_relative_score, 5);

        assert_eq!(stored, MATE_SCORE);
        assert_eq!(score_from_tt(stored, 2), MATE_SCORE - 2);
    }

    #[test]
    fn tt_score_normalization_round_trips_losing_mates_across_plies() {
        let root_relative_score = -MATE_SCORE + 5;
        let stored = score_to_tt(root_relative_score, 5);

        assert_eq!(stored, -MATE_SCORE);
        assert_eq!(score_from_tt(stored, 2), -MATE_SCORE + 2);
    }

    #[test]
    fn tt_probe_returns_exact_deep_enough_entry() {
        let mut table = TranspositionTable::new(16);

        table.store(tt_entry_with_bound(42, 3, 75, TtBound::Exact));

        assert_eq!(probe_tt(&table, 42, 3, -10, 10, 0), Some(75));
    }

    #[test]
    fn tt_probe_ignores_shallow_entry() {
        let mut table = TranspositionTable::new(16);

        table.store(tt_entry_with_bound(42, 2, 75, TtBound::Exact));

        assert_eq!(probe_tt(&table, 42, 3, -10, 10, 0), None);
    }

    #[test]
    fn tt_probe_returns_lower_bound_only_when_it_cuts_off() {
        let mut table = TranspositionTable::new(16);

        table.store(tt_entry_with_bound(42, 3, 75, TtBound::Lower));

        assert_eq!(probe_tt(&table, 42, 3, 0, 50, 0), Some(75));
        assert_eq!(probe_tt(&table, 42, 3, 0, 90, 0), None);
    }

    #[test]
    fn tt_probe_returns_upper_bound_only_when_it_cuts_off() {
        let mut table = TranspositionTable::new(16);

        table.store(tt_entry_with_bound(42, 3, 25, TtBound::Upper));

        assert_eq!(probe_tt(&table, 42, 3, 30, 100, 0), Some(25));
        assert_eq!(probe_tt(&table, 42, 3, 10, 100, 0), None);
    }

    #[test]
    fn negamax_stores_completed_exact_result() {
        let mut pos = parse_pos("4k3/8/8/8/8/8/8/4KQ2 w - - 0 1");
        let key = pos.zobrist;
        let mut history = Vec::new();
        let mut nodes = 0;
        let mut table = TranspositionTable::new(64);

        let score = negamax(
            &mut pos,
            &mut history,
            1,
            -INF,
            INF,
            0,
            &mut nodes,
            None,
            &mut table,
        )
        .expect("search should finish");
        let entry = table.probe(key).expect("expected a stored TT entry");

        assert_eq!(entry.depth, 1);
        assert_eq!(entry.bound, TtBound::Exact);
        assert_eq!(score_from_tt(entry.score, 0), score);
        assert!(entry.best_move.is_some());
    }

    #[test]
    fn negamax_does_not_store_immediate_draw_result() {
        let mut pos = parse_pos("4k3/8/8/8/8/8/8/4KQ2 w - - 100 1");
        let key = pos.zobrist;
        let mut history = Vec::new();
        let mut nodes = 0;
        let mut table = TranspositionTable::new(64);

        let score = negamax(
            &mut pos,
            &mut history,
            1,
            -INF,
            INF,
            0,
            &mut nodes,
            None,
            &mut table,
        );

        assert_eq!(score, Some(0));
        assert_eq!(table.probe(key), None);
    }

    #[test]
    fn depth_zero_returns_eval_and_no_best_move() {
        let mut pos = parse_pos("4k3/8/8/8/8/8/8/4KQ2 w - - 0 1");
        let expected = evaluate(&pos);

        let result = search(&mut pos, &[], SearchLimits::depth(0));

        assert_eq!(result.best_move, None);
        assert_eq!(result.score, expected);
        assert!(result.nodes >= 1);
    }

    #[test]
    fn depth_zero_uses_quiescence_score() {
        let mut pos = parse_pos("4k3/8/8/8/8/3r4/8/3QK3 w - - 0 1");
        let static_eval = evaluate(&pos);

        let result = search(&mut pos, &[], SearchLimits::depth(0));

        assert_eq!(result.best_move, None);
        assert!(result.score > static_eval);
        assert!(result.nodes > 1);
    }

    #[test]
    fn checkmate_terminal_score_is_negative_for_mated_side() {
        let mut pos = parse_pos("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1");

        let result = search(&mut pos, &[], SearchLimits::depth(3));

        assert_eq!(result.best_move, None);
        assert_eq!(result.score, -MATE_SCORE);
    }

    #[test]
    fn depth_one_prefers_winning_capture() {
        let mut pos = parse_pos("4k3/8/8/8/8/5q2/8/4KQ2 w - - 0 1");

        let result = search(&mut pos, &[], SearchLimits::depth(1));

        assert_eq!(
            result.best_move,
            Some(
                Move::new(Square::from_file_rank(5, 0), Square::from_file_rank(5, 2))
                    .with_capture()
            )
        );
        assert!(result.score > 800);
    }

    #[test]
    fn depth_one_avoids_protected_capture_after_quiescence_recapture() {
        let mut pos = parse_pos("4k3/8/6b1/8/8/3r4/8/3QK3 w - - 0 1");
        let poisoned_capture =
            Move::new(Square::from_file_rank(3, 0), Square::from_file_rank(3, 2)).with_capture();

        let result = search(&mut pos, &[], SearchLimits::depth(1));

        assert_ne!(result.best_move, Some(poisoned_capture));
        assert!(result.score < 500);
    }

    #[test]
    fn tactical_depth_one_counts_quiescence_nodes() {
        let mut pos = parse_pos("4k3/8/6b1/8/8/3r4/8/3QK3 w - - 0 1");
        let mut legal_moves = Vec::new();
        generate_legal(&mut pos, &mut legal_moves);
        let old_leaf_only_nodes = 1 + legal_moves.len() as u64;

        let result = search(&mut pos, &[], SearchLimits::depth(1));

        assert!(result.nodes > old_leaf_only_nodes);
    }

    #[test]
    fn search_preserves_position_after_completion() {
        assert_search_preserves_position(
            "r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPPB1PPP/R2QKB1R w KQkq c6 0 1",
            3,
        );
    }

    #[test]
    fn stalemate_terminal_score_is_draw() {
        let mut pos = parse_pos("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");

        let result = search(&mut pos, &[], SearchLimits::depth(3));

        assert_eq!(result.best_move, None);
        assert_eq!(result.score, 0);
    }

    #[test]
    fn fifty_move_rule_scores_as_draw() {
        let mut pos = parse_pos("4k3/8/8/8/8/8/8/4KQ2 w - - 100 1");

        let result = search(&mut pos, &[], SearchLimits::depth(3));

        assert_eq!(result.best_move, None);
        assert_eq!(result.score, 0);
    }

    #[test]
    fn threefold_repetition_scores_as_draw_with_supplied_history() {
        let mut pos = parse_pos("4k1n1/8/8/8/8/8/8/4K1N1 w - - 8 5");
        let history = vec![
            state_with_zobrist(0),
            state_with_zobrist(pos.zobrist),
            state_with_zobrist(1),
            state_with_zobrist(pos.zobrist),
        ];

        let result = search(&mut pos, &history, SearchLimits::depth(3));

        assert_eq!(result.best_move, None);
        assert_eq!(result.score, 0);
    }

    #[test]
    fn mate_in_one_selects_move_that_checkmates() {
        let mut pos = parse_pos("7k/6pp/6QK/8/8/8/8/8 w - - 0 1");

        let result = search(&mut pos, &[], SearchLimits::depth(1));
        let best_move = result.best_move.expect("expected a mate-in-one move");
        let state = pos.make_move(best_move);

        assert!(in_check(&pos, pos.side_to_move));
        assert!(has_no_legal_moves(&mut pos));

        pos.unmake_move(best_move, state);
    }

    #[test]
    fn mate_score_prefers_shorter_mates() {
        let mut mated_at_root = parse_pos("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1");
        let mut mate_in_one = parse_pos("7k/6pp/6QK/8/8/8/8/8 w - - 0 1");

        let root_score = search(&mut mated_at_root, &[], SearchLimits::depth(3)).score;
        let mate_in_one_score = search(&mut mate_in_one, &[], SearchLimits::depth(3)).score;

        assert_eq!(root_score, -MATE_SCORE);
        assert_eq!(mate_in_one_score, MATE_SCORE - 1);
    }

    #[test]
    fn move_ordering_prioritizes_captures_then_promotions() {
        let quiet = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3));
        let knight_promotion = Move::with_promotion(
            Square::from_file_rank(0, 6),
            Square::from_file_rank(0, 7),
            PieceKind::Knight,
        );
        let queen_promotion = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(1, 7),
            PieceKind::Queen,
        );
        let capture =
            Move::new(Square::from_file_rank(2, 2), Square::from_file_rank(2, 5)).with_capture();
        let mut moves = [quiet, knight_promotion, queen_promotion, capture];

        order_moves(&mut moves);

        assert_eq!(moves[0], capture);
        assert_eq!(moves[1], queen_promotion);
        assert_eq!(moves[2], knight_promotion);
        assert_eq!(moves[3], quiet);
    }

    #[test]
    fn hash_move_ordering_prioritizes_hash_move() {
        let quiet = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3));
        let queen_promotion = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(1, 7),
            PieceKind::Queen,
        );
        let capture =
            Move::new(Square::from_file_rank(2, 2), Square::from_file_rank(2, 5)).with_capture();
        let mut moves = [capture, quiet, queen_promotion];

        order_moves_with_hash_move(&mut moves, Some(quiet));

        assert_eq!(moves[0], quiet);
    }

    #[test]
    fn hash_move_ordering_ignores_missing_hash_move() {
        let quiet = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3));
        let queen_promotion = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(1, 7),
            PieceKind::Queen,
        );
        let capture =
            Move::new(Square::from_file_rank(2, 2), Square::from_file_rank(2, 5)).with_capture();
        let missing_hash_move =
            Move::new(Square::from_file_rank(0, 1), Square::from_file_rank(0, 2));
        let mut without_hash = [quiet, queen_promotion, capture];
        let mut with_missing_hash = without_hash;

        order_moves(&mut without_hash);
        order_moves_with_hash_move(&mut with_missing_hash, Some(missing_hash_move));

        assert_eq!(with_missing_hash, without_hash);
    }

    #[test]
    fn hash_move_ordering_preserves_regular_order_for_other_moves() {
        let quiet = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3));
        let knight_promotion = Move::with_promotion(
            Square::from_file_rank(0, 6),
            Square::from_file_rank(0, 7),
            PieceKind::Knight,
        );
        let queen_promotion = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(1, 7),
            PieceKind::Queen,
        );
        let capture =
            Move::new(Square::from_file_rank(2, 2), Square::from_file_rank(2, 5)).with_capture();
        let mut moves = [quiet, knight_promotion, queen_promotion, capture];

        order_moves_with_hash_move(&mut moves, Some(quiet));

        assert_eq!(moves[0], quiet);
        assert_eq!(moves[1], capture);
        assert_eq!(moves[2], queen_promotion);
        assert_eq!(moves[3], knight_promotion);
    }

    #[test]
    fn quiescence_ordering_prefers_more_valuable_victim() {
        let pos = parse_pos("4k3/7p/8/3q3Q/4P3/8/8/4K3 w - - 0 1");
        let queen_takes_pawn =
            Move::new(Square::from_file_rank(7, 4), Square::from_file_rank(7, 6)).with_capture();
        let pawn_takes_queen =
            Move::new(Square::from_file_rank(4, 3), Square::from_file_rank(3, 4)).with_capture();
        let mut moves = [queen_takes_pawn, pawn_takes_queen];

        order_quiescence_moves(&pos, &mut moves);

        assert_eq!(moves[0], pawn_takes_queen);
        assert_eq!(moves[1], queen_takes_pawn);
    }

    #[test]
    fn quiescence_ordering_prefers_less_valuable_attacker_for_same_victim() {
        let pos = parse_pos("4k3/8/8/3r3Q/4P3/8/8/4K3 w - - 0 1");
        let queen_takes_rook =
            Move::new(Square::from_file_rank(7, 4), Square::from_file_rank(3, 4)).with_capture();
        let pawn_takes_rook =
            Move::new(Square::from_file_rank(4, 3), Square::from_file_rank(3, 4)).with_capture();
        let mut moves = [queen_takes_rook, pawn_takes_rook];

        order_quiescence_moves(&pos, &mut moves);

        assert_eq!(moves[0], pawn_takes_rook);
        assert_eq!(moves[1], queen_takes_rook);
    }

    #[test]
    fn quiescence_ordering_scores_en_passant_victim_from_capture_square() {
        let pos = parse_pos("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        let en_passant =
            Move::new(Square::from_file_rank(4, 4), Square::from_file_rank(3, 5)).with_en_passant();

        assert_eq!(pos.piece_at(en_passant.to()), None);
        assert_eq!(capture_victim(&pos, en_passant), Some(PieceKind::Pawn));
        assert_eq!(
            quiescence_move_order_score(&pos, en_passant),
            10_000 + material_value_for_ordering(PieceKind::Pawn) * 10
                - material_value_for_ordering(PieceKind::Pawn)
        );
    }

    #[test]
    fn quiescence_ordering_preserves_quiet_promotion_value_ordering() {
        let pos = parse_pos("4k3/PP6/8/8/8/8/8/4K3 w - - 0 1");
        let knight_promotion = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(1, 7),
            PieceKind::Knight,
        );
        let queen_promotion = Move::with_promotion(
            Square::from_file_rank(0, 6),
            Square::from_file_rank(0, 7),
            PieceKind::Queen,
        );
        let mut moves = [knight_promotion, queen_promotion];

        order_quiescence_moves(&pos, &mut moves);

        assert_eq!(moves[0], queen_promotion);
        assert_eq!(moves[1], knight_promotion);
    }

    #[test]
    fn quiescence_ordering_adds_promotion_value_to_capture_promotions() {
        let pos = parse_pos("r3k3/1P6/8/3r4/4P3/8/8/4K3 w - - 0 1");
        let pawn_takes_rook =
            Move::new(Square::from_file_rank(4, 3), Square::from_file_rank(3, 4)).with_capture();
        let promoted_pawn_takes_rook = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(0, 7),
            PieceKind::Queen,
        )
        .with_capture();
        let mut moves = [pawn_takes_rook, promoted_pawn_takes_rook];

        order_quiescence_moves(&pos, &mut moves);

        assert_eq!(moves[0], promoted_pawn_takes_rook);
        assert_eq!(moves[1], pawn_takes_rook);
    }

    #[test]
    fn quiescence_moves_include_captures_and_promotions() {
        let capture =
            Move::new(Square::from_file_rank(2, 2), Square::from_file_rank(2, 5)).with_capture();
        let en_passant =
            Move::new(Square::from_file_rank(4, 4), Square::from_file_rank(5, 5)).with_en_passant();
        let quiet_promotion = Move::with_promotion(
            Square::from_file_rank(0, 6),
            Square::from_file_rank(0, 7),
            PieceKind::Queen,
        );
        let capture_promotion = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(2, 7),
            PieceKind::Knight,
        )
        .with_capture();

        assert!(is_quiescence_move(capture));
        assert!(is_quiescence_move(en_passant));
        assert!(is_quiescence_move(quiet_promotion));
        assert!(is_quiescence_move(capture_promotion));
    }

    #[test]
    fn quiescence_moves_exclude_ordinary_quiets_and_castling() {
        let quiet = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3));
        let castling =
            Move::new(Square::from_file_rank(4, 0), Square::from_file_rank(6, 0)).with_castling();

        assert!(!is_quiescence_move(quiet));
        assert!(!is_quiescence_move(castling));
    }

    #[test]
    fn quiescence_stand_pat_returns_static_eval_in_quiet_position() {
        let mut pos = parse_pos("4k3/8/8/8/8/8/8/4KQ2 w - - 0 1");
        let mut history = Vec::new();
        let mut nodes = 0;
        let expected = evaluate(&pos);

        let score = quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None);

        assert_eq!(score, Some(expected));
        assert_eq!(nodes, 1);
    }

    #[test]
    fn quiescence_scores_draw_as_zero() {
        let mut pos = parse_pos("4k3/8/8/8/8/8/8/4KQ2 w - - 100 1");
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None);

        assert_eq!(score, Some(0));
        assert_eq!(nodes, 1);
    }

    #[test]
    fn quiescence_returns_none_after_deadline() {
        let mut pos = parse_pos("4k3/8/8/8/8/8/8/4KQ2 w - - 0 1");
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(
            &mut pos,
            &mut history,
            -INF,
            INF,
            0,
            &mut nodes,
            Some(Instant::now() - Duration::from_millis(1)),
        );

        assert_eq!(score, None);
        assert_eq!(nodes, 0);
    }

    #[test]
    fn quiescence_preserves_position() {
        let mut pos = parse_pos("4k3/8/8/8/4r3/8/4K3/8 w - - 0 1");
        let before = pos.clone();
        let mut history = Vec::new();
        let mut nodes = 0;

        quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None);

        assert_eq!(pos, before);
    }

    #[test]
    fn quiescence_searches_winning_capture() {
        let mut pos = parse_pos("4k3/8/8/8/8/3r4/8/3QK3 w - - 0 1");
        let static_eval = evaluate(&pos);
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None)
            .expect("quiescence should finish");

        assert!(score > static_eval);
        assert!(nodes > 1);
    }

    #[test]
    fn quiescence_sees_recapture_after_capture() {
        let mut pos = parse_pos("4k3/8/8/8/3q4/8/8/4K1B1 w - - 0 1");
        let static_eval = evaluate(&pos);
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None)
            .expect("quiescence should finish");

        assert!(score > static_eval);
        assert!(nodes > 1);
    }

    #[test]
    fn quiescence_searches_quiet_promotion() {
        let mut pos = parse_pos("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
        let static_eval = evaluate(&pos);
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None)
            .expect("quiescence should finish");

        assert!(score > static_eval);
        assert!(score > 800);
        assert!(nodes > 1);
    }

    #[test]
    fn quiescence_preserves_position_after_capture_chain() {
        let mut pos = parse_pos("4k3/8/8/8/3q4/8/8/4K1B1 w - - 0 1");
        let before = pos.clone();
        let mut history = Vec::new();
        let mut nodes = 0;

        quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None);

        assert_eq!(pos, before);
        assert!(history.is_empty());
    }

    #[test]
    fn quiescence_does_not_stand_pat_in_check() {
        let mut pos = parse_pos("4k3/8/8/8/8/4r3/4K3/8 w - - 0 1");
        let static_eval = evaluate(&pos);
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None)
            .expect("quiescence should finish");

        assert!(score > static_eval);
        assert!(nodes > 1);
    }

    #[test]
    fn quiescence_searches_quiet_check_evasion() {
        let mut pos = parse_pos("4k3/8/8/8/4r3/8/4K3/8 w - - 0 1");
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 0, &mut nodes, None);

        assert!(score.is_some());
        assert!(nodes > 1);
    }

    #[test]
    fn quiescence_scores_stalemate_as_draw() {
        let mut pos = parse_pos("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 4, &mut nodes, None);

        assert_eq!(score, Some(0));
    }

    #[test]
    fn quiescence_scores_checkmate_with_ply_adjustment() {
        let mut pos = parse_pos("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1");
        let mut history = Vec::new();
        let mut nodes = 0;

        let score = quiescence(&mut pos, &mut history, -INF, INF, 4, &mut nodes, None);

        assert_eq!(score, Some(-MATE_SCORE + 4));
    }

    #[test]
    fn timed_search_returns_legal_move_from_startpos() {
        let mut pos = parse_pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let deadline = Instant::now() + Duration::from_millis(10);

        let result = search(&mut pos, &[], SearchLimits::timed(64, deadline));

        assert!(result.best_move.is_some());
    }

    #[test]
    fn timed_search_falls_back_after_expired_deadline() {
        let mut pos = parse_pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let deadline = Instant::now() - Duration::from_millis(1);

        let result = search(&mut pos, &[], SearchLimits::timed(64, deadline));

        assert!(result.best_move.is_some());
        assert_eq!(result.nodes, 1);
    }

    #[test]
    fn timed_search_terminal_position_returns_no_move() {
        let mut pos = parse_pos("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1");
        let deadline = Instant::now() + Duration::from_millis(10);

        let result = search(&mut pos, &[], SearchLimits::timed(64, deadline));

        assert_eq!(result.best_move, None);
        assert_eq!(result.score, -MATE_SCORE);
    }

    fn state_with_zobrist(zobrist: u64) -> State {
        State {
            castling: crate::core::position::CastlingRights::EMPTY,
            ep_square: None,
            halfmove_clock: 0,
            captured: None,
            zobrist,
        }
    }
}
