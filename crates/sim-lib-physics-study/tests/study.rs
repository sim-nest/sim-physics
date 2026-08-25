use sim_incremental_core::QueryBudgets;
use sim_kernel::{
    Consistency, ContentId, Cx, DefaultFactory, Error, EvalFabric, EvalReply, EvalRequest, Expr,
    Factory, Result as KernelResult, Symbol, testing::bare_cx,
};
use sim_lib_physics_influence::{InfluenceAudit, SinkKind, StudyGraph, StudyNode, Transform};
use sim_lib_physics_study::*;
use sim_shape::{AnyShape, shape_value};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

fn id(byte: u8) -> ContentId {
    ContentId::from_bytes(Symbol::qualified("core", "sha256"), [byte; 32])
}
fn set(values: impl IntoIterator<Item = usize>) -> BTreeSet<usize> {
    values.into_iter().collect()
}

fn plan(sampler: SamplerPolicy, partitions: PartitionPlan) -> StudyPlan {
    StudyPlan {
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
        outputs: vec!["x".into()],
        limits: StudyLimits {
            max_points: 32,
            max_failures: 8,
            deadline: None,
        },
        seed: 7,
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
                inclusive_bounds: (1.0, 3.0),
                spacing: Spacing::Linear,
            },
            ParameterAxis {
                name: "rate".into(),
                quantity_shape: shape_value(
                    Symbol::qualified("quantity", "rate"),
                    Arc::new(AnyShape),
                ),
                inclusive_bounds: (1.0, 100.0),
                spacing: Spacing::Logarithmic,
            },
        ],
        sampler,
        boundary_injections: vec![
            BoundaryInjection {
                label: "lower".into(),
                values: vec![1.0, 1.0],
            },
            BoundaryInjection {
                label: "exact-interior".into(),
                values: vec![2.0, 10.0],
            },
        ],
        partitions,
        untested_regions: vec![UntestedRegion {
            label: "negative-mass".into(),
            reason: "outside model domain".into(),
        }],
    }
}

#[test]
fn grid_boundaries_partitions_and_gaps_are_exact() {
    let p = plan(
        SamplerPolicy::Grid { counts: vec![2, 2] },
        PartitionPlan {
            fit: set([0, 1]),
            selection: set([2]),
            test: set([3, 4]),
        },
    );
    let d = p.design().unwrap();
    assert_eq!(d.points.len(), 5); // lower deduplicates; exact interior is appended
    assert!(
        d.boundary_evidence
            .iter()
            .any(|(label, _, duplicate)| label == "lower" && *duplicate)
    );
    assert_eq!(d.coverage.untested_regions[0].label, "negative-mass");
    assert_eq!(
        d.points
            .iter()
            .filter(|p| p.partition == Partition::Test)
            .count(),
        2
    );
}

#[test]
fn latin_and_sobol_replay_with_sampler_evidence() {
    for sampler in [
        SamplerPolicy::Latin { points: 4 },
        SamplerPolicy::Sobol {
            points: 4,
            skip: 1,
            scramble: Scramble::DigitalShift,
        },
    ] {
        let p = plan(
            sampler,
            PartitionPlan {
                fit: set(0..6),
                selection: set([]),
                test: set([]),
            },
        );
        let a = p.design().unwrap();
        let b = p.design().unwrap();
        assert_eq!(
            a.points.iter().map(|p| &p.id).collect::<Vec<_>>(),
            b.points.iter().map(|p| &p.id).collect::<Vec<_>>()
        );
        assert!(a.coverage.sampler.is_some());
    }
}

struct Expression;
impl PointExpression for Expression {
    fn expression(&self, _: &StudyPlan, p: &StudyPoint) -> KernelResult<Expr> {
        Ok(Expr::String(format!("point-{}", p.ordinal)))
    }
}
struct ScriptedFabric {
    calls: Mutex<usize>,
    provider_label: &'static str,
}
impl EvalFabric for ScriptedFabric {
    fn realize(&self, _: &mut Cx, _: EvalRequest) -> KernelResult<EvalReply> {
        let mut n = self.calls.lock().unwrap();
        let current = *n;
        *n += 1;
        match current {
            1 => Err(Error::Eval("capability refused".into())),
            2 => Err(Error::Eval("interrupted by caller".into())),
            3 => Err(Error::Eval("contact lost".into())),
            _ => Ok(EvalReply {
                value: DefaultFactory.string(self.provider_label.into())?,
                diagnostics: vec![],
                trace: None,
            }),
        }
    }
}

#[test]
fn ordinary_fabric_retains_complete_refused_interrupted_and_unknown() {
    let p = plan(
        SamplerPolicy::Grid { counts: vec![2, 2] },
        PartitionPlan {
            fit: set(0..5),
            selection: set([]),
            test: set([]),
        },
    );
    let d = p.design().unwrap();
    let mut cx = bare_cx();
    let results = realize_sweep(
        &mut cx,
        &ScriptedFabric {
            calls: Mutex::new(0),
            provider_label: "cpu",
        },
        &p,
        &d,
        &Expression,
        id(9),
    );
    assert_eq!(results.len(), 5);
    assert!(matches!(
        results[1].outcome,
        PointOutcome::Incomplete(IncompleteOutcome::Refused(_))
    ));
    assert!(matches!(
        results[2].outcome,
        PointOutcome::Incomplete(IncompleteOutcome::Interrupted(_))
    ));
    assert!(matches!(
        results[3].outcome,
        PointOutcome::Incomplete(IncompleteOutcome::Unknown(_))
    ));
    assert!(
        StudyResults::new(p.id.clone(), results)
            .retry_safe
            .is_empty()
    );
}

#[test]
fn placement_parity_replay_envelopes_and_energy_separation() {
    let p = plan(
        SamplerPolicy::Grid { counts: vec![1, 1] },
        PartitionPlan {
            fit: set([]),
            selection: set(0..3),
            test: set([]),
        },
    );
    let d = p.design().unwrap();
    let run = |provider| {
        let mut cx = bare_cx();
        realize_sweep(
            &mut cx,
            &ScriptedFabric {
                calls: Mutex::new(0),
                provider_label: "same",
            },
            &p,
            &d,
            &Expression,
            provider,
        )
    };
    let a = run(id(8));
    let replay = run(id(8));
    let other = run(id(9));
    assert!(exact_replay(&a[0], &replay[0]));
    assert!(!exact_replay(&a[0], &other[0]));
    let envelope = |provider| ProviderEnvelope {
        provider_id: provider,
        absolute_tolerance: 1e-6,
        relative_tolerance: 1e-4,
    };
    assert!(compare_provider_values(1.0, 1.00005, &envelope(id(8)), &envelope(id(9))).equivalent);
    let graph = StudyGraph::build(
        [StudyNode {
            id: 1,
            location: "clean choice".into(),
            transform: Transform::Sink(SinkKind::Selection),
        }],
        [],
        [],
    )
    .unwrap();
    let proof = InfluenceAudit::complete(graph, QueryBudgets::unlimited())
        .unwrap()
        .prepare(1)
        .unwrap();
    assert!(select(&proof, a.iter(), |_| Some(1.0)).is_some());
    let observations = detect(&ObservationInput {
        energy_before: Some(2.0),
        energy_after: Some(3.0),
        residual: Some(0.2),
        previous: Some(-1.0),
        current: Some(1.0),
        event_changed: true,
        topology_changed: true,
        initial_delta: Some(-0.3),
        boundary_value: true,
        slope_before: Some(-1.0),
        slope_after: Some(1.0),
        method_delta: Some(-0.1),
        tolerance_delta: Some(0.02),
        precision_delta: Some(-0.01),
    });
    assert_eq!(observations.energy_store_change, Some(1.0));
    assert!(
        observations.sign_reversal
            && observations.zero_crossing
            && observations.fold
            && observations.event_transition
            && observations.topology_transition
    );
}

#[test]
fn overlap_missing_gaps_and_work_limits_fail_closed() {
    let mut overlap = plan(
        SamplerPolicy::Grid { counts: vec![1, 1] },
        PartitionPlan {
            fit: set([0]),
            selection: set([0]),
            test: set([]),
        },
    );
    assert!(matches!(
        overlap.validate(),
        Err(PlanError::PartitionOverlap(0))
    ));
    overlap.partitions = PartitionPlan {
        fit: set([0]),
        selection: set([]),
        test: set([]),
    };
    overlap.untested_regions.clear();
    assert!(matches!(
        overlap.validate(),
        Err(PlanError::Empty("known untested regions"))
    ));
    let missing = plan(
        SamplerPolicy::Grid { counts: vec![2, 2] },
        PartitionPlan {
            fit: set([0]),
            selection: set([]),
            test: set([]),
        },
    );
    assert!(matches!(
        missing.design(),
        Err(PlanError::MissingPartition(_))
    ));
}
