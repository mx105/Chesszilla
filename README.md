# Chesszilla

Chesszilla is a small chess engine written in Rust. It speaks UCI, generates
legal moves, searches positions, and tries to pick a decent move without leaning
on any external crates.

This is mostly a learning project. A lot of the ideas came from the
[Chess Programming Wiki](https://www.chessprogramming.org/Main_Page), which is
an incredible resource if you are trying to figure out how chess engines work.
The code here is my own implementation, but the wiki deserves a lot of credit
for the concepts behind it.

## What It Can Do

- Run as a UCI chess engine
- Parse FEN positions
- Handle legal move generation
- Make and unmake moves
- Support castling, en passant, promotions, checks, checkmate, and stalemate
- Use bitboards for position representation
- Keep Zobrist hashes for positions
- Detect threefold repetition
- Detect fifty-move-rule draws
- Run perft-style move generation checks
- Search with negamax and alpha-beta pruning
- Use iterative deepening for timed searches
- Use quiescence search for noisy positions
- Order moves in a basic way
- Store positions in a transposition table
- Evaluate with material and simple piece-square tables
- Cover the core engine behavior with unit tests

## UCI Commands

Chesszilla supports the UCI commands needed for basic GUI use:

- `uci`
- `isready`
- `ucinewgame`
- `position startpos`
- `position startpos moves ...`
- `position fen ...`
- `position fen ... moves ...`
- `go depth N`
- `go movetime N`
- `go wtime N btime N winc N binc N movestogo N`
- `stop`
- `quit`

Example:

```text
uci
isready
position startpos moves e2e4 e7e5
go depth 4
```

## Build

```bash
cargo build --release
```

The engine binary will be here:

```bash
target/release/chesszilla
```

## Run

```bash
cargo run --release
```

You can type UCI commands by hand, or point a chess GUI at
`target/release/chesszilla`.

## Test

```bash
cargo test
```

The tests cover move generation, special moves, FEN parsing, make/unmake
correctness, Zobrist hashing, UCI parsing, search behavior, draw detection, and
some known perft positions.

## Things I Might Add Later

- Better UCI `info` output during search
- Configurable UCI options, like hash size
- Better time management
- Principal variation tracking
- Aspiration windows
- Null-move pruning
- Late move reductions
- Killer move and history heuristics
- Static exchange evaluation
- Better quiet-move ordering
- Tapered middlegame/endgame evaluation
- Pawn structure evaluation
- Mobility, king safety, passed pawn, and rook file evaluation
- Self-play testing and rough Elo tracking
- Any other Chess Wiki rabbit hole I end up in

## Credits

Most of the engine ideas I used here came from the
[Chess Programming Wiki](https://www.chessprogramming.org/Main_Page), especially
the pages around board representation, move generation, Zobrist hashing,
alpha-beta search, quiescence search, transposition tables, and evaluation.

## License

MIT
