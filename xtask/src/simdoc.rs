//! Thin simdoc launcher: defers to the shared sim-tooling Card encoder.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(args: Vec<String>) -> Result<(), String> {
    let program = args.first().map(String::as_str).unwrap_or("xtask");
    if args.get(1).map(String::as_str) != Some("simdoc") {
        return Err(format!("usage: {program} simdoc [--check]"));
    }

    let root = env::current_dir().map_err(|err| format!("current dir: {err}"))?;
    let mut command = if let Some(binary) = locate_built_sim_tooling(&root) {
        let mut command = Command::new(binary);
        command.arg("simdoc");
        command
    } else {
        let manifest = locate_sim_tooling_manifest(&root)?;
        let mut command = Command::new("cargo");
        command.args(["run", "--manifest-path"]);
        command.arg(manifest);
        command.args(["--quiet", "--", "simdoc"]);
        command
    };
    configure_local_origin(&mut command);
    command.arg("--repo-root");
    command.arg(&root);
    for arg in args.iter().skip(2) {
        command.arg(arg);
    }

    let status = command
        .status()
        .map_err(|err| format!("run shared simdoc encoder: {err}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("shared simdoc encoder failed with status {status}"))
}

fn configure_local_origin(command: &mut Command) {
    command.env("GIT_CONFIG_COUNT", "1");
    command.env("GIT_CONFIG_KEY_0", "remote.origin.url");
    command.env(
        "GIT_CONFIG_VALUE_0",
        "https://github.com/sim-nest/sim-physics",
    );
}

fn locate_built_sim_tooling(repo_root: &Path) -> Option<PathBuf> {
    let binary = repo_root.parent()?.join("sim-tooling/target/debug/xtask");
    binary.is_file().then_some(binary)
}

fn locate_sim_tooling_manifest(repo_root: &Path) -> Result<PathBuf, String> {
    if let Ok(path) = env::var("SIMDOC_TOOLING_MANIFEST") {
        return Ok(PathBuf::from(path));
    }
    let sibling = repo_root
        .parent()
        .unwrap_or(repo_root)
        .join("sim-tooling")
        .join("Cargo.toml");
    if sibling.is_file() {
        return Ok(sibling);
    }
    Err("set SIMDOC_TOOLING_MANIFEST to the sim-tooling Cargo.toml".to_owned())
}
