use sim_kernel::Lib;
use sim_lib_physics_runtime::{PhysicsRuntimeLib, surface_symbols};

#[test]
fn manifest_and_projection_have_the_same_unique_exports() {
    let manifest = PhysicsRuntimeLib.manifest();
    let symbols = surface_symbols();
    assert_eq!(manifest.exports.len(), symbols.len());
    let mut sorted = symbols.iter().map(ToString::to_string).collect::<Vec<_>>();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), symbols.len());
}
