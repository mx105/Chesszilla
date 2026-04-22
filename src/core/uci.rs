#![allow(dead_code)]

use crate::core::game::Game;
use crate::core::search::{SearchLimits, search};
use crate::core::types::Color;
use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};

pub const TIMED_SEARCH_MAX_DEPTH: u8 = 64;
const CLOCK_SAFETY_MS: u64 = 10;
const DEFAULT_MOVES_TO_GO: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciCommand {
    Uci,
    IsReady,
    UciNewGame,
    Quit,
    Stop,
    Position(PositionCommand),
    Go(GoCommand),
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionCommand {
    pub base: PositionBase,
    pub moves: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionBase {
    Startpos,
    Fen(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GoCommand {
    pub depth: Option<u8>,
    pub movetime_ms: Option<u64>,
    pub wtime_ms: Option<u64>,
    pub btime_ms: Option<u64>,
    pub winc_ms: Option<u64>,
    pub binc_ms: Option<u64>,
    pub movestogo: Option<u32>,
}

pub fn parse_command(line: &str) -> UciCommand {
    let mut parts = line.split_whitespace();
    match parts.next() {
        Some("uci") => UciCommand::Uci,
        Some("isready") => UciCommand::IsReady,
        Some("ucinewgame") => UciCommand::UciNewGame,
        Some("quit") => UciCommand::Quit,
        Some("stop") => UciCommand::Stop,
        Some("position") => parse_position(parts.collect()),
        Some("go") => parse_go(parts.collect()),
        _ => UciCommand::Ignore,
    }
}

pub fn run_uci<R: BufRead, W: Write>(input: R, output: &mut W) -> io::Result<()> {
    let mut game = Game::startpos();

    for line in input.lines() {
        let line = line?;
        match parse_command(&line) {
            UciCommand::Uci => {
                writeln!(output, "id name chesszilla")?;
                writeln!(output, "id author chesszilla")?;
                writeln!(output, "uciok")?;
            }
            UciCommand::IsReady => {
                writeln!(output, "readyok")?;
            }
            UciCommand::UciNewGame => {
                game = Game::startpos();
            }
            UciCommand::Position(position) => {
                if let Ok(next_game) = game_from_position_command(position) {
                    game = next_game;
                }
            }
            UciCommand::Go(go) => {
                if let Some(limits) =
                    search_limits_for_go(&go, game.pos.side_to_move, Instant::now())
                {
                    let result = search(&mut game.pos, &game.history, limits);
                    if let Some(best_move) = result.best_move {
                        writeln!(output, "bestmove {}", best_move.to_uci())?;
                    } else {
                        writeln!(output, "bestmove 0000")?;
                    }
                }
            }
            UciCommand::Stop | UciCommand::Ignore => {}
            UciCommand::Quit => break,
        }
        output.flush()?;
    }

    Ok(())
}

fn game_from_position_command(command: PositionCommand) -> Result<Game, ()> {
    let mut game = match command.base {
        PositionBase::Startpos => Game::startpos(),
        PositionBase::Fen(fen) => Game::from_fen(&fen).map_err(|_| ())?,
    };

    for mv in command.moves {
        game.apply_uci_move(&mv).map_err(|_| ())?;
    }

    Ok(game)
}

pub fn search_limits_for_go(
    go: &GoCommand,
    side_to_move: Color,
    now: Instant,
) -> Option<SearchLimits> {
    if let Some(depth) = go.depth {
        return Some(SearchLimits::depth(depth));
    }

    let budget = time_budget_for_go(go, side_to_move)?;
    Some(SearchLimits::timed(TIMED_SEARCH_MAX_DEPTH, now + budget))
}

pub fn time_budget_for_go(go: &GoCommand, side_to_move: Color) -> Option<Duration> {
    if let Some(movetime_ms) = go.movetime_ms {
        return Some(Duration::from_millis(apply_safety_margin(movetime_ms)));
    }

    let remaining_ms = match side_to_move {
        Color::White => go.wtime_ms?,
        Color::Black => go.btime_ms?,
    };
    let increment_ms = match side_to_move {
        Color::White => go.winc_ms.unwrap_or(0),
        Color::Black => go.binc_ms.unwrap_or(0),
    };
    let moves_to_go = u64::from(go.movestogo.unwrap_or(DEFAULT_MOVES_TO_GO as u32)).max(1);
    let budget_ms = remaining_ms / moves_to_go + increment_ms;

    if budget_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(apply_safety_margin(budget_ms)))
    }
}

fn apply_safety_margin(ms: u64) -> u64 {
    ms.saturating_sub(CLOCK_SAFETY_MS).max(1)
}

fn parse_position(tokens: Vec<&str>) -> UciCommand {
    let Some(first) = tokens.first() else {
        return UciCommand::Ignore;
    };

    let (base, rest) = match *first {
        "startpos" => (PositionBase::Startpos, &tokens[1..]),
        "fen" => {
            if tokens.len() < 7 {
                return UciCommand::Ignore;
            }
            let fen = tokens[1..7].join(" ");
            (PositionBase::Fen(fen), &tokens[7..])
        }
        _ => return UciCommand::Ignore,
    };

    let moves = if rest.is_empty() {
        Vec::new()
    } else if rest[0] == "moves" {
        rest[1..].iter().map(|mv| (*mv).to_owned()).collect()
    } else {
        return UciCommand::Ignore;
    };

    UciCommand::Position(PositionCommand { base, moves })
}

fn parse_go(tokens: Vec<&str>) -> UciCommand {
    let mut go = GoCommand::default();
    let mut index = 0;

    while index < tokens.len() {
        let key = tokens[index];
        index += 1;

        match key {
            "depth" => {
                let Some(value) = parse_next::<u8>(&tokens, &mut index) else {
                    return UciCommand::Ignore;
                };
                go.depth = Some(value);
            }
            "movetime" => {
                let Some(value) = parse_next::<u64>(&tokens, &mut index) else {
                    return UciCommand::Ignore;
                };
                go.movetime_ms = Some(value);
            }
            "wtime" => {
                let Some(value) = parse_next::<u64>(&tokens, &mut index) else {
                    return UciCommand::Ignore;
                };
                go.wtime_ms = Some(value);
            }
            "btime" => {
                let Some(value) = parse_next::<u64>(&tokens, &mut index) else {
                    return UciCommand::Ignore;
                };
                go.btime_ms = Some(value);
            }
            "winc" => {
                let Some(value) = parse_next::<u64>(&tokens, &mut index) else {
                    return UciCommand::Ignore;
                };
                go.winc_ms = Some(value);
            }
            "binc" => {
                let Some(value) = parse_next::<u64>(&tokens, &mut index) else {
                    return UciCommand::Ignore;
                };
                go.binc_ms = Some(value);
            }
            "movestogo" => {
                let Some(value) = parse_next::<u32>(&tokens, &mut index) else {
                    return UciCommand::Ignore;
                };
                go.movestogo = Some(value);
            }
            _ => {}
        }
    }

    if go == GoCommand::default() {
        UciCommand::Ignore
    } else {
        UciCommand::Go(go)
    }
}

fn parse_next<T: std::str::FromStr>(tokens: &[&str], index: &mut usize) -> Option<T> {
    let value = tokens.get(*index)?.parse().ok()?;
    *index += 1;
    Some(value)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Cursor;

    fn run_script(script: &str) -> String {
        let input = Cursor::new(script.as_bytes());
        let mut output = Vec::new();

        run_uci(input, &mut output).unwrap();

        String::from_utf8(output).unwrap()
    }

    #[test]
    fn parses_simple_commands() {
        assert_eq!(parse_command("uci"), UciCommand::Uci);
        assert_eq!(parse_command("isready"), UciCommand::IsReady);
        assert_eq!(parse_command("ucinewgame"), UciCommand::UciNewGame);
        assert_eq!(parse_command("stop"), UciCommand::Stop);
        assert_eq!(parse_command("quit"), UciCommand::Quit);
    }

    #[test]
    fn parses_position_startpos() {
        assert_eq!(
            parse_command("position startpos"),
            UciCommand::Position(PositionCommand {
                base: PositionBase::Startpos,
                moves: Vec::new(),
            })
        );
    }

    #[test]
    fn parses_position_startpos_with_moves() {
        assert_eq!(
            parse_command("position startpos moves e2e4 e7e5"),
            UciCommand::Position(PositionCommand {
                base: PositionBase::Startpos,
                moves: vec!["e2e4".to_owned(), "e7e5".to_owned()],
            })
        );
    }

    #[test]
    fn parses_position_fen() {
        assert_eq!(
            parse_command("position fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
            UciCommand::Position(PositionCommand {
                base: PositionBase::Fen(
                    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned()
                ),
                moves: Vec::new(),
            })
        );
    }

    #[test]
    fn parses_position_fen_with_moves() {
        assert_eq!(
            parse_command("position fen 4k3/8/8/8/8/8/8/4K3 w - - 0 1 moves e1e2"),
            UciCommand::Position(PositionCommand {
                base: PositionBase::Fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_owned()),
                moves: vec!["e1e2".to_owned()],
            })
        );
    }

    #[test]
    fn parses_go_depth() {
        assert_eq!(
            parse_command("go depth 4"),
            UciCommand::Go(GoCommand {
                depth: Some(4),
                ..GoCommand::default()
            })
        );
    }

    #[test]
    fn parses_go_movetime() {
        assert_eq!(
            parse_command("go movetime 250"),
            UciCommand::Go(GoCommand {
                movetime_ms: Some(250),
                ..GoCommand::default()
            })
        );
    }

    #[test]
    fn parses_go_clock_fields() {
        assert_eq!(
            parse_command("go wtime 60000 btime 55000 winc 1000 binc 500 movestogo 20"),
            UciCommand::Go(GoCommand {
                wtime_ms: Some(60000),
                btime_ms: Some(55000),
                winc_ms: Some(1000),
                binc_ms: Some(500),
                movestogo: Some(20),
                ..GoCommand::default()
            })
        );
    }

    #[test]
    fn malformed_or_unknown_commands_are_ignored() {
        for line in [
            "",
            "debug on",
            "position",
            "position fen 8/8/8/8/8/8/8/8 w - -",
            "position startpos unexpected",
            "go",
            "go depth nope",
            "go movetime",
        ] {
            assert_eq!(parse_command(line), UciCommand::Ignore);
        }
    }

    #[test]
    fn depth_go_command_maps_to_fixed_depth_search() {
        let go = GoCommand {
            depth: Some(3),
            ..GoCommand::default()
        };

        assert_eq!(
            search_limits_for_go(&go, Color::White, Instant::now()),
            Some(SearchLimits::depth(3))
        );
    }

    #[test]
    fn movetime_budget_uses_safety_margin() {
        let go = GoCommand {
            movetime_ms: Some(50),
            ..GoCommand::default()
        };

        assert_eq!(
            time_budget_for_go(&go, Color::White),
            Some(Duration::from_millis(40))
        );
    }

    #[test]
    fn clock_budget_uses_side_to_move_time_increment_and_movestogo() {
        let go = GoCommand {
            wtime_ms: Some(60000),
            btime_ms: Some(30000),
            winc_ms: Some(1000),
            binc_ms: Some(500),
            movestogo: Some(20),
            ..GoCommand::default()
        };

        assert_eq!(
            time_budget_for_go(&go, Color::White),
            Some(Duration::from_millis(3990))
        );
        assert_eq!(
            time_budget_for_go(&go, Color::Black),
            Some(Duration::from_millis(1990))
        );
    }

    #[test]
    fn clock_budget_defaults_to_thirty_moves() {
        let go = GoCommand {
            wtime_ms: Some(30000),
            ..GoCommand::default()
        };

        assert_eq!(
            time_budget_for_go(&go, Color::White),
            Some(Duration::from_millis(990))
        );
    }

    #[test]
    fn runner_responds_to_uci_and_isready() {
        let output = run_script("uci\nisready\nquit\n");

        assert!(output.contains("id name chesszilla\n"));
        assert!(output.contains("uciok\n"));
        assert!(output.contains("readyok\n"));
    }

    #[test]
    fn runner_searches_startpos_at_fixed_depth() {
        let output = run_script("position startpos\ngo depth 1\nquit\n");

        assert!(output.lines().any(|line| line.starts_with("bestmove ")));
        assert!(!output.contains("bestmove 0000"));
    }

    #[test]
    fn runner_returns_null_move_for_terminal_position() {
        let output = run_script("position fen 7k/6Q1/6K1/8/8/8/8/8 b - - 0 1\ngo depth 1\nquit\n");

        assert!(output.contains("bestmove 0000\n"));
    }

    #[test]
    fn runner_ignores_illegal_position_move_and_keeps_responding() {
        let output = run_script("position startpos moves e2e5\nisready\nquit\n");

        assert!(output.contains("readyok\n"));
    }
}
