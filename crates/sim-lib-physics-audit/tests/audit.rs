// conformance: independent stored-energy audit contract
use sim_kernel::ClaimKind;
use sim_lib_physics_audit::*;
use sim_lib_physics_power::{
    ConjugatePort, ImpulseKind, PositiveDirection, PowerPair, ReductionPlan, SegmentEvidence,
};

fn id(value: &str) -> ContentId {
    ContentId::new(value).unwrap()
}

struct Spring {
    evaluator: ContentId,
    model: ContentId,
}
impl StoreEvaluator for Spring {
    fn evaluator_id(&self) -> &ContentId {
        &self.evaluator
    }
    fn model_id(&self) -> &ContentId {
        &self.model
    }
    fn evaluate(&self, state: &ConstitutiveState) -> Result<StoredEnergy, AuditError> {
        Ok(StoredEnergy(
            0.5 * state.coordinates[0] * state.coordinates[0],
        ))
    }
}

fn endpoint(evaluator: &Spring, state: &str, coordinate: f64) -> StoreEvaluation {
    evaluate_store(
        evaluator,
        &ConstitutiveState {
            state_id: id(state),
            coordinates: vec![coordinate],
        },
        vec![id("evidence/state")],
    )
    .unwrap()
}

fn work(value: f64, port_id: &str) -> PortWork {
    let port = ConjugatePort {
        boundary: BoundaryId::new("system").unwrap(),
        port: sim_lib_physics_power::PortRef::new(port_id).unwrap(),
        pair: PowerPair::force_velocity(),
        positive: PositiveDirection::IntoBoundary,
    };
    PortWork {
        port,
        source: id(&format!("trajectory/{port_id}")),
        span: WorkSpan {
            start: 0.0,
            end: 1.0,
        },
        plan: ReductionPlan::Trapezoid { intervals: 8 },
        segments: vec![SegmentEvidence {
            start: 0.0,
            end: 1.0,
            method: "trapezoid".into(),
            evaluations: 9,
            signed_work: value,
            uncertainty: 1e-8,
        }],
        signed_work: value,
        uncertainty: 1e-8,
    }
}

fn uncertainty() -> UncertaintyLanes {
    UncertaintyLanes {
        numerical: vec![NumericalUncertainty(UncertaintyComponent {
            id: id("uncertainty/numerical"),
            quantity: "energy".into(),
            lower: -1e-8,
            upper: 1e-8,
            evidence: vec![id("evidence/quadrature")],
        })],
        model: vec![],
        measurement: vec![],
        combination: CombinationRule::RetainSeparate,
    }
}

fn input(start: f64, end: f64, works: Vec<PortWork>) -> AuditInput {
    let evaluator = Spring {
        evaluator: id("evaluator/spring-v1"),
        model: id("model/hooke-v1"),
    };
    AuditInput {
        boundary: BoundaryId::new("system").unwrap(),
        span: WorkSpan {
            start: 0.0,
            end: 1.0,
        },
        start_store: endpoint(&evaluator, "state/start", start),
        end_store: endpoint(&evaluator, "state/end", end),
        port_work: works,
        event_transfers: vec![],
        reduction_method: id("method/neumaier"),
        method_evidence: vec![],
        solver_residuals: vec![SolverResidual {
            value: 1e-12,
            normalization: Some(1.0),
            threshold_eligible: true,
            evidence: vec![id("evidence/solver")],
        }],
        quadrature_residuals: vec![],
        model_residuals: vec![],
        uncertainty: uncertainty(),
        event_status: EventStatus {
            complete: true,
            evidence: vec![id("evidence/events")],
        },
        evidence: vec![id("evidence/audit")],
    }
}

#[test]
fn lossless_and_explicitly_dissipative_balances_are_visible() {
    let lossless = AuditRecord::new(input(0.0, 2.0, vec![work(2.0, "drive")])).unwrap();
    assert_eq!(lossless.residuals.energy_balance.value, 0.0);
    let dissipative = AuditRecord::new(input(
        0.0,
        2.0,
        vec![work(3.0, "drive"), work(-1.0, "dissipation")],
    ))
    .unwrap();
    assert_eq!(dissipative.reduction.port_terms, vec![3.0, -1.0]);
    assert_eq!(dissipative.residuals.energy_balance.value, 0.0);
}

#[test]
fn omitted_port_wrong_sign_and_perturbed_store_are_not_hidden() {
    let omitted = AuditRecord::new(input(0.0, 2.0, vec![work(3.0, "drive")])).unwrap();
    assert_eq!(omitted.residuals.energy_balance.value, -1.0);
    let wrong_sign = AuditRecord::new(input(0.0, 2.0, vec![work(-2.0, "drive")])).unwrap();
    assert_eq!(wrong_sign.residuals.energy_balance.value, 4.0);
    let perturbed = AuditRecord::new(input(0.0, 3.0, vec![work(2.0, "drive")])).unwrap();
    assert_eq!(perturbed.residuals.energy_balance.value, 2.5);
    assert_eq!(perturbed.claim().kind, ClaimKind::Observed);
    assert_ne!(omitted.id, perturbed.id);
}

#[test]
fn impulses_model_error_and_measurement_uncertainty_stay_separate() {
    let mut audit = input(0.0, 2.0, vec![work(1.0, "drive")]);
    audit.event_transfers.push(ImpulseTransfer {
        id: id("event/impact"),
        port: sim_lib_physics_power::PortRef::new("drive").unwrap(),
        time: 0.5,
        signed_energy: 1.0,
        kind: ImpulseKind::Mechanical,
        state_before: id("state/pre-impact"),
        state_after: id("state/post-impact"),
        constitutive_source: id("model/impact"),
    });
    audit.model_residuals.push(ModelResidual {
        value: 0.2,
        normalization: None,
        threshold_eligible: false,
        evidence: vec![id("evidence/model-comparison")],
    });
    audit
        .uncertainty
        .measurement
        .push(MeasurementUncertainty(UncertaintyComponent {
            id: id("uncertainty/sensor"),
            quantity: "energy".into(),
            lower: -0.3,
            upper: 0.4,
            evidence: vec![id("evidence/calibration")],
        }));
    let record = AuditRecord::new(audit).unwrap();
    assert_eq!(record.residuals.energy_balance.value, 0.0);
    assert_eq!(record.residuals.model[0].value, 0.2);
    assert_eq!(record.uncertainty.measurement[0].0.upper, 0.4);
}

#[test]
fn small_solver_residual_can_coexist_with_large_energy_residual() {
    let record = AuditRecord::new(input(0.0, 4.0, vec![work(1.0, "drive")])).unwrap();
    assert_eq!(record.residuals.solver[0].value, 1e-12);
    assert_eq!(record.residuals.energy_balance.value, 7.0);
    assert_eq!(EnergyBalanceResidual::QUANTITY, "energy");
    assert_ne!(SolverResidual::KIND, EnergyBalanceResidual::KIND);
}

#[test]
fn independent_endpoints_and_uncertainty_semantics_fail_closed() {
    let mut same = input(0.0, 0.0, vec![]);
    same.end_store.state_id = same.start_store.state_id.clone();
    assert_eq!(
        AuditRecord::new(same),
        Err(AuditError::EndpointsNotIndependent)
    );
    let mut unlike = input(0.0, 2.0, vec![work(2.0, "drive")]);
    unlike.uncertainty.combination = CombinationRule::WorstCaseSameQuantity;
    unlike
        .uncertainty
        .measurement
        .push(MeasurementUncertainty(UncertaintyComponent {
            id: id("uncertainty/time"),
            quantity: "time".into(),
            lower: -0.1,
            upper: 0.1,
            evidence: vec![],
        }));
    assert_eq!(AuditRecord::new(unlike), Err(AuditError::UnlikeUncertainty));
}
