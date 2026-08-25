use std::{fs, path::Path};

#[test]
fn generic_dataflow_engine_remains_in_sim_incremental_core() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root.ancestors().nth(2).unwrap();
    let policy = fs::read_to_string(repo_root.join("dataflow-ownership.toml")).unwrap();
    assert!(policy.contains("generic_owner = \"sim-incremental-core\""));
    assert!(policy.contains("classification = \"domain-transfer-rules\""));
    let source = fs::read_to_string(crate_root.join("src/lib.rs")).unwrap();
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
