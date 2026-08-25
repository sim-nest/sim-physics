use sim_lib_physics_adapter::*;

fn id(s: &str) -> StableIdentity {
    StableIdentity::new(s).unwrap()
}
fn model() -> AdaptedModel {
    AdaptedModel {
        model_id: id("model/1"),
        state_id: id("state/1"),
        boundary_id: id("boundary/1"),
        boundary_complete: true,
        ports: vec![],
        stores: vec![id("store/energy")],
        events: vec![id("event/solve")],
        observations: vec![Observation {
            id: id("observation/energy"),
            kind: "si:energy".into(),
            dimension: ENERGY,
            value: 2.0,
            unit: "J".into(),
            origin: DataOrigin::Modeled,
        }],
        influences: vec![id("state/temperature")],
        model_evidence: vec!["analytic-model".into()],
        solver_evidence: vec!["residual=0".into()],
    }
}

#[test]
fn accepts_complete_records_but_refuses_missing_lumped_port() {
    assert_eq!(model().validate(), Ok(()));
    assert_eq!(
        model().validate_lumped_audit(),
        Err(AdapterRefusal::MissingPort)
    );
}
#[test]
fn separates_origin_and_evidence_and_propagates_influences() {
    let mut value = model();
    value.observations[0].origin = DataOrigin::Observed;
    assert_eq!(value.influences, vec![id("state/temperature")]);
    assert_ne!(value.observations[0].origin, DataOrigin::Modeled);
    value.solver_evidence.clear();
    assert_eq!(value.validate(), Err(AdapterRefusal::EvidenceNotSeparated));
}
