use std::{fs, path::Path};

#[test]
fn generic_dataflow_engine_remains_in_sim_incremental_core() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Generated constellation workspaces materialize manifests but symlink
    // source trees. Resolve the owning source before locating repo policy.
    let source_path = fs::canonicalize(crate_root.join("src/lib.rs")).unwrap();
    let owning_crate_root = source_path.parent().unwrap().parent().unwrap();
    let repo_root = owning_crate_root.ancestors().nth(2).unwrap();
    let policy = fs::read_to_string(repo_root.join("dataflow-ownership.toml")).unwrap();
    assert!(policy.contains("generic_owner = \"sim-incremental-core\""));
    assert!(policy.contains("classification = \"domain-transfer-rules\""));
    let source = fs::read_to_string(source_path).unwrap();
    for forbidden in [
        "VecDeque",
        "BinaryHeap",
        "struct Worklist",
        "struct CompletionProof",
        "struct Continuation",
    ] {
        assert!(
            !source.contains(forbidden),
            "alternate dataflow owner found: {forbidden}"
        );
    }
}
