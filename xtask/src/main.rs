#![forbid(unsafe_code)]
//! Repository automation wrapper for sim-femm generated documentation.

mod simdoc;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let result = match args.get(1).map(String::as_str) {
        Some("check-recipes") => check_recipes(),
        _ => simdoc::run(args),
    };
    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn check_recipes() -> Result<(), String> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let recipes = [
        "recipes/00-overview/boundary-event/Cargo.toml",
        "recipes/01-power/switched-two-port/Cargo.toml",
        "recipes/02-audit/stored-energy/Cargo.toml",
        "recipes/03-proof/certified-refinement/Cargo.toml",
        "recipes/04-influence/refused-selection/Cargo.toml",
    ];
    for manifest in recipes {
        let status = std::process::Command::new("cargo")
            .args(["run", "--quiet", "--manifest-path", manifest])
            .current_dir(&root)
            .status()
            .map_err(|error| format!("run {manifest}: {error}"))?;
        if !status.success() {
            return Err(format!("recipe failed: {manifest}"));
        }
    }
    Ok(())
}
