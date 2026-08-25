#![forbid(unsafe_code)]
//! Repository automation wrapper for sim-femm generated documentation.

mod simdoc;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let result = simdoc::run(args);
    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
