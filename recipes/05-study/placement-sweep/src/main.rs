use sim_kernel::{Consistency, ContentId, Symbol};
use sim_lib_physics_study::*;
use sim_shape::{AnyShape, shape_value};
use std::{collections::BTreeSet, sync::Arc};

fn id(byte: u8) -> ContentId {
    ContentId::from_bytes(Symbol::qualified("core", "sha256"), [byte; 32])
}

fn main() {
    let all = (0..7).collect::<BTreeSet<_>>();
    let plan = StudyPlan {
        id: id(1),
        model_id: id(2),
        boundary_event_graph_id: id(3),
        initial_state_id: id(4),
        method_plan_ids: vec![id(5)],
        refinement_plan_ids: vec![id(6)],
        audit_policy: AuditPolicy {
            energy_after_selection_only: true,
            record_residuals: true,
        },
        influence_policy: InfluencePolicy {
            require_clean_selection: true,
        },
        outputs: vec!["displacement".into()],
        limits: StudyLimits {
            max_points: 16,
            max_failures: 2,
            deadline: None,
        },
        seed: 42,
        placement: PlacementRequest {
            target: Symbol::qualified("compute", "automatic"),
            consistency: Consistency::LocalFirst,
            required_capabilities: vec![],
        },
        axes: vec![
            ParameterAxis {
                name: "mass".into(),
                quantity_shape: shape_value(
                    Symbol::qualified("quantity", "mass"),
                    Arc::new(AnyShape),
                ),
                inclusive_bounds: (1.0, 2.0),
                spacing: Spacing::Linear,
            },
            ParameterAxis {
                name: "stiffness".into(),
                quantity_shape: shape_value(
                    Symbol::qualified("quantity", "stiffness"),
                    Arc::new(AnyShape),
                ),
                inclusive_bounds: (10.0, 20.0),
                spacing: Spacing::Linear,
            },
        ],
        sampler: SamplerPolicy::Grid { counts: vec![2, 3] },
        boundary_injections: vec![BoundaryInjection {
            label: "resonance-boundary".into(),
            values: vec![1.5, 15.0],
        }],
        partitions: PartitionPlan {
            fit: all,
            selection: BTreeSet::new(),
            test: BTreeSet::new(),
        },
        untested_regions: vec![UntestedRegion {
            label: "nonlinear-damping".into(),
            reason: "separate constitutive model".into(),
        }],
    };
    let design = plan.design().expect("reviewed sweep");
    println!(
        "points={} boundaries={:?} gaps={}",
        design.points.len(),
        design.boundary_evidence,
        design.coverage.untested_regions.len()
    );
}
