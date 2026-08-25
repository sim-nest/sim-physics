use sim_incremental_core::{
    QueryBudgets, ValueFingerprint,
    dataflow::{
        AdmittedTransfer, Boundary, DataflowGraph, DataflowProgress, EdgeClass, EdgeSpec,
        FixpointEngine, GraphDirection, NodeSpec, TransferPolicy,
    },
};
use sim_lib_physics_influence::*;
fn node(id: u64, transform: Transform) -> StudyNode {
    StudyNode {
        id,
        location: format!("study:{id}"),
        transform,
    }
}
fn edge(id: u64, source: u64, target: u64) -> StudyEdge {
    StudyEdge { id, source, target }
}
fn chain(middle: Transform, source: Option<InfluenceSource>) -> StudyGraph {
    StudyGraph::build(
        [
            node(1, Transform::Input),
            node(2, middle),
            node(3, Transform::Sink(SinkKind::Selection)),
        ],
        [edge(1, 1, 2), edge(2, 2, 3)],
        source.map(|s| (1, s)),
    )
    .unwrap()
}
#[test]
fn direct_and_dimension_laundering_are_refused_with_paths() {
    for transform in [
        Transform::Arithmetic,
        Transform::DimensionChange,
        Transform::DomainAdapter,
    ] {
        let audit = InfluenceAudit::complete(
            chain(transform, Some(InfluenceSource::Energy)),
            QueryBudgets::unlimited(),
        )
        .unwrap();
        let AuditError::Refused(refusal) = audit.prepare(3).unwrap_err() else {
            panic!("must refuse")
        };
        assert!(matches!(refusal.influence, Influence::EnergyObserved(_)));
        assert!(refusal.path.iter().any(|(node, _)| *node == 1));
    }
}
#[test]
fn every_influence_lane_survives_arithmetic() {
    for source in [
        InfluenceSource::Energy,
        InfluenceSource::Work,
        InfluenceSource::Power,
        InfluenceSource::Passivity,
        InfluenceSource::BalanceResidual,
        InfluenceSource::Derived,
    ] {
        assert!(matches!(
            InfluenceAudit::complete(
                chain(Transform::Arithmetic, Some(source)),
                QueryBudgets::unlimited()
            )
            .unwrap()
            .prepare(3),
            Err(AuditError::Refused(_))
        ));
    }
}
#[test]
fn dimensionless_ratio_branch_join_loop_and_opaque_fail_closed() {
    let graph = StudyGraph::build(
        [
            node(1, Transform::Input),
            node(2, Transform::DimensionChange),
            node(3, Transform::Branch),
            node(4, Transform::Arithmetic),
            node(5, Transform::Join),
            node(6, Transform::Loop),
            node(7, Transform::Sink(SinkKind::Ranking)),
            node(8, Transform::Opaque),
            node(9, Transform::Sink(SinkKind::Control)),
        ],
        [
            edge(1, 1, 2),
            edge(2, 2, 3),
            edge(3, 3, 4),
            edge(4, 3, 5),
            edge(5, 4, 5),
            edge(6, 5, 6),
            edge(7, 6, 5),
            edge(8, 6, 7),
            edge(9, 8, 9),
        ],
        [(1, InfluenceSource::Energy)],
    )
    .unwrap();
    let audit = InfluenceAudit::complete(graph, QueryBudgets::unlimited()).unwrap();
    assert!(matches!(audit.prepare(7), Err(AuditError::Refused(_))));
    assert!(matches!(
        audit.prepare(9),
        Err(AuditError::Refused(Refusal {
            influence: Influence::Unknown,
            ..
        }))
    ));
}
#[test]
fn checked_native_preserves_clean_and_clean_sink_is_prepared() {
    let audit = InfluenceAudit::complete(
        chain(Transform::CheckedNative, None),
        QueryBudgets::unlimited(),
    )
    .unwrap();
    let input = audit.prepare(3).unwrap();
    fn native_api(value: &impl CleanSelection) -> u64 {
        value.selection_input().proof_identity()
    }
    assert_eq!(native_api(&input), audit.proof_identity());
}
#[test]
fn incremental_input_edit_invalidates_the_sink_proof() {
    let clean = InfluenceAudit::complete(
        chain(Transform::Arithmetic, None),
        QueryBudgets::unlimited(),
    )
    .unwrap();
    let clean_id = clean.proof_identity();
    assert!(clean.prepare(3).is_ok());
    let changed = clean
        .complete_incremental(chain(Transform::Arithmetic, Some(InfluenceSource::Power)))
        .unwrap();
    assert_ne!(changed.proof_identity(), clean_id);
    assert!(matches!(changed.prepare(3), Err(AuditError::Refused(_))));
}
#[test]
fn bounded_run_refuses_without_a_completion_proof() {
    let error = InfluenceAudit::complete(
        chain(Transform::Arithmetic, Some(InfluenceSource::Energy)),
        QueryBudgets::new(1, 100, 100, 100),
    )
    .err()
    .unwrap();
    assert!(matches!(error, AuditError::Dataflow(_)));
}

#[derive(Clone, Copy)]
struct Identity;
impl TransferPolicy<Influence> for Identity {
    fn fingerprint(&self) -> ValueFingerprint {
        ValueFingerprint::new(42)
    }
    fn policy_size(&self) -> usize {
        0
    }
    fn transfer(&self, state: &Influence) -> Influence {
        state.clone()
    }
}

#[test]
fn generic_budget_suspension_resumes_without_losing_influence() {
    let graph = DataflowGraph::build(
        [
            NodeSpec {
                id: 1,
                location: "source",
                boundary: Boundary::Input,
            },
            NodeSpec {
                id: 2,
                location: "convert",
                boundary: Boundary::Internal,
            },
            NodeSpec {
                id: 3,
                location: "sink",
                boundary: Boundary::Output,
            },
        ],
        [
            EdgeSpec {
                id: 1,
                source: 1,
                target: 2,
                class: EdgeClass::<()>::Data,
                direction: GraphDirection::Forward,
            },
            EdgeSpec {
                id: 2,
                source: 2,
                target: 3,
                class: EdgeClass::<()>::Data,
                direction: GraphDirection::Forward,
            },
        ],
    )
    .unwrap();
    let samples = [
        Influence::Clean,
        Influence::from_source(InfluenceSource::Energy),
        Influence::Unknown,
    ];
    let transfer = AdmittedTransfer::admit(Identity, &samples).unwrap();
    let DataflowProgress::Suspended(continuation) = FixpointEngine::start_resumable(
        &graph,
        &transfer,
        Influence::Clean,
        [(1, Influence::from_source(InfluenceSource::Energy))],
        1,
        8,
    )
    .unwrap() else {
        panic!("one visit must suspend")
    };
    let DataflowProgress::Complete(solution) = FixpointEngine::resume(
        &graph,
        &transfer,
        &Influence::Clean,
        [(1, Influence::from_source(InfluenceSource::Energy))],
        continuation,
        8,
    )
    .unwrap() else {
        panic!("resume must complete")
    };
    assert!(matches!(
        solution.state(&3),
        Some(Influence::EnergyObserved(_))
    ));
}

#[test]
fn structural_graph_edit_forces_a_fresh_proof() {
    let clean = InfluenceAudit::complete(
        chain(Transform::Arithmetic, None),
        QueryBudgets::unlimited(),
    )
    .unwrap();
    let old = clean.proof_identity();
    let edited = StudyGraph::build(
        [
            node(1, Transform::Input),
            node(2, Transform::Arithmetic),
            node(4, Transform::DomainAdapter),
            node(3, Transform::Sink(SinkKind::Selection)),
        ],
        [edge(1, 1, 2), edge(3, 2, 4), edge(4, 4, 3)],
        [],
    )
    .unwrap();
    let refreshed = clean.complete_incremental(edited).unwrap();
    assert_ne!(refreshed.proof_identity(), old);
    assert!(refreshed.prepare(3).is_ok());
}
