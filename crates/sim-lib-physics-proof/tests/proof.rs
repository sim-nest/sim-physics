use sim_lib_numbers_interval::{
    CertifiedInterval, EstimateInterval, ExactRational, RationalInterval, ThresholdVerdict,
};
use sim_lib_physics_proof::*;

fn certified(lo: i128, hi: i128) -> CertifiedInterval {
    RationalInterval::new(
        ExactRational::new(lo, 1).unwrap(),
        ExactRational::new(hi, 1).unwrap(),
    )
    .unwrap()
    .certify()
    .unwrap()
}

#[test]
fn event_mesh_is_one_sided_and_rejects_out_of_span() {
    let mesh = event_exact_mesh(
        (0.0, 2.0),
        &[0.0, 2.0],
        &[LocatedEvent {
            id: "switch".into(),
            at: 1.0,
        }],
    )
    .unwrap();
    assert_eq!((mesh[1].side, mesh[2].side), (Side::Left, Side::Right));
    assert!(
        event_exact_mesh(
            (0.0, 2.0),
            &[],
            &[LocatedEvent {
                id: "bad".into(),
                at: 3.0
            }]
        )
        .is_err()
    );
}

#[test]
fn sign_reversal_and_non_monotone_refinement_remain_visible() {
    let reversal = compare_levels(2.0, -1.0, "coarse", "fine");
    assert_eq!(reversal.signed_delta, -3.0);
    assert_eq!(
        (
            reversal.coarse_evidence.as_str(),
            reversal.fine_evidence.as_str()
        ),
        ("coarse", "fine")
    );
    let next = compare_levels(-1.0, 1.5, "fine", "finer");
    assert!(reversal.signed_delta < 0.0 && next.signed_delta > 0.0);
}

#[test]
fn independent_failure_defeats_false_single_level_pass() {
    let values = GateValues {
        solver: Some(0.01),
        quadrature: Some(0.02),
        model: Some(0.01),
        balance: Some(0.01),
        absolute_work: Some(2.0),
        relative_work: Some(0.01),
    };
    let limits = GateValues {
        solver: Some(0.1),
        quadrature: Some(0.1),
        model: Some(0.1),
        balance: Some(0.1),
        absolute_work: Some(1.0),
        relative_work: Some(0.1),
    };
    let report = apply_thresholds(&values, &limits).unwrap();
    assert_eq!(report.absolute_work, Some(false));
    assert!(!report.all_applicable_pass);
}

#[test]
fn estimate_refused_while_certificate_is_definite() {
    let estimate = certified_verdict(
        VerdictInput::Estimate(EstimateInterval::new(0.0, 1.0).unwrap()),
        VerdictInput::Estimate(EstimateInterval::new(2.0, 3.0).unwrap()),
    );
    assert!(matches!(estimate, ThresholdVerdict::Unresolved(_)));
    let (value, threshold) = (certified(0, 1), certified(2, 3));
    assert_eq!(
        certified_verdict(
            VerdictInput::Certified(&value),
            VerdictInput::Certified(&threshold)
        ),
        ThresholdVerdict::Below
    );
}

#[test]
fn continuation_reports_method_disagreement_and_model_sensitivity() {
    let solver = ContinuationEvidence {
        unchanged_study_input: true,
        time_refinement_converged: Some(false),
        precision_change_resolved: Some(false),
        methods_agree: Some(false),
        models_agree: Some(true),
    };
    assert_eq!(
        classify_continuation(&solver),
        ContinuationOutcome::SolverLimited
    );
    let model = ContinuationEvidence {
        models_agree: Some(false),
        ..solver
    };
    assert_eq!(
        classify_continuation(&model),
        ContinuationOutcome::ModelSensitive
    );
}

#[test]
fn proof_identity_is_reproducible_and_measurement_is_separate() {
    let verdict = ThresholdVerdict::Below;
    let a = ProofRecord::new(
        "ode:model-v2",
        "params:ci-7",
        vec!["method:b".into(), "method:a".into()],
        &verdict,
        PhysicalValidation::NotAssessed,
    )
    .unwrap();
    let b = ProofRecord::new(
        "ode:model-v2",
        "params:ci-7",
        vec!["method:a".into(), "method:b".into()],
        &verdict,
        PhysicalValidation::Validated {
            measurement_evidence: "lab:42".into(),
        },
    )
    .unwrap();
    assert_eq!(a.proof_id, b.proof_id);
    assert_ne!(a.physical_validation, b.physical_validation);
}
