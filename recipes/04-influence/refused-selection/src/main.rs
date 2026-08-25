use sim_incremental_core::QueryBudgets;
use sim_lib_physics_influence::*;
fn main() {
    let graph = StudyGraph::build(
        [
            StudyNode {
                id: 1,
                location: "stored energy".into(),
                transform: Transform::Input,
            },
            StudyNode {
                id: 2,
                location: "divide by time".into(),
                transform: Transform::DimensionChange,
            },
            StudyNode {
                id: 3,
                location: "rank designs".into(),
                transform: Transform::Sink(SinkKind::Ranking),
            },
        ],
        [
            StudyEdge {
                id: 1,
                source: 1,
                target: 2,
            },
            StudyEdge {
                id: 2,
                source: 2,
                target: 3,
            },
        ],
        [(1, InfluenceSource::Energy)],
    )
    .unwrap();
    let audit = InfluenceAudit::complete(graph, QueryBudgets::unlimited()).unwrap();
    let AuditError::Refused(refusal) = audit.prepare(3).unwrap_err() else {
        panic!("energy laundering was accepted")
    };
    println!(
        "refused {} at {} via {:?}",
        refusal.sink, refusal.location, refusal.path
    );
}
