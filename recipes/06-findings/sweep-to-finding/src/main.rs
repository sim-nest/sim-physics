use sim_kernel::{ContentId, Symbol};
use sim_lib_physics_findings::{EvidenceRef, FindingRecord, Observation, Sign, UncertaintyLane};

fn id(seed: u8) -> ContentId {
    ContentId::from_bytes(Symbol::new("core/sha256"), [seed; 32])
}

fn main() {
    let finding = FindingRecord::open(
        Observation {
            magnitude: 0.031,
            sign: Sign::Positive,
            boundary_id: id(1),
            study_id: id(2),
            occurrence_conditions: "high-load boundary point".into(),
            residual_class: "energy-balance".into(),
            model: "motor-v3".into(),
            parameter_region: "load >= 0.9".into(),
            uncertainty: vec![UncertaintyLane {
                name: "quadrature".into(),
                absolute: 0.002,
            }],
            checks_performed: vec!["orthogonal refinement".into()],
        },
        vec![EvidenceRef {
            scheme: "study".into(),
            id: "sweep-42/point-91".into(),
        }],
        "reviewed sweep import",
    );
    println!("{:?}", finding.content_id().expect("valid finding"));
}
