use sim_lib_physics_proof::{LocatedEvent, RUNTIME_EXPORTS, event_exact_mesh};

fn main() {
    let mesh = event_exact_mesh(
        (0.0, 1.0),
        &[0.0, 1.0],
        &[LocatedEvent {
            id: "contact".into(),
            at: 0.5,
        }],
    )
    .unwrap();
    println!(
        "event-exact points: {}; exports: {}",
        mesh.len(),
        RUNTIME_EXPORTS.join(", ")
    );
}
