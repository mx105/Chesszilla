use std::io::{self, BufReader};

mod core;

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    core::uci::run_uci(BufReader::new(stdin.lock()), &mut stdout)
}
