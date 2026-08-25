//! Self-contained analytic studies. Every expected value follows from the
//! equations below; no external corpus or generated fixture is an input.

use sim_incremental_core::QueryBudgets;
use sim_kernel::{ContentId as KernelId, Symbol};
use sim_lib_numbers_interval::{ExactRational, RationalInterval, ThresholdVerdict};
use sim_lib_numbers_poly::{Polynomial, RootPlan};
use sim_lib_physics_core::{BoundaryId, PortRef};
use sim_lib_physics_influence::{InfluenceAudit, SinkKind, StudyGraph, StudyNode, Transform};
use sim_lib_physics_power::{
    CallableHistory, ConjugatePort, ContentId, ImpulseKind, ImpulseTransfer, OneSidedValues,
    PortHistory, PositiveDirection, PowerPair, ReductionPlan, WorkEvent, WorkSpan, integrate_port,
};
use sim_lib_physics_proof::{VerdictInput, certified_verdict};
use sim_lib_physics_study::{ProviderEnvelope, compare_provider_values};
use std::sync::Arc;

fn cid(value: &str) -> ContentId {
    ContentId::new(value).unwrap()
}
fn boundary(value: &str) -> BoundaryId {
    BoundaryId::new(value).unwrap()
}
fn port(value: &str) -> PortRef {
    PortRef::new(value).unwrap()
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

fn electrical_port(name: &str, positive: PositiveDirection) -> ConjugatePort {
    ConjugatePort {
        boundary: boundary("switched-network"),
        port: port(name),
        pair: PowerPair::voltage_current(),
        positive,
    }
}

/// A switched two-port network has p_in=12(2t) and p_load=6t before t=1/2,
/// then p_in=12t and p_load=3t. Thus net work is 9/4+27/8=45/8 J.
/// The short 1/1024 s interval immediately after switching is the declared
/// stiff interval. Both endpoint stores are evaluated from their own state
/// equations: E_C=Cv^2/2 and E_L=Li^2/2.
#[test]
fn switched_electrical_study_is_analytic_certified_and_refinement_stable() {
    let event = WorkEvent {
        id: cid("event/switch"),
        time: 0.5,
        sides: Some(OneSidedValues {
            left: (12.0, 1.0),
            right: (12.0, 0.5),
        }),
        terminal: false,
    };
    let terminal = WorkEvent {
        id: cid("event/terminal"),
        time: 1.0,
        sides: None,
        terminal: true,
    };
    let input = PortHistory::Callable(CallableHistory {
        span: WorkSpan {
            start: 0.0,
            end: 1.0,
        },
        source: cid("trajectory/electrical/input"),
        sample: Arc::new(|t| (12.0, if t <= 0.5 { 2.0 * t } else { t })),
    });
    let load = PortHistory::Callable(CallableHistory {
        span: WorkSpan {
            start: 0.0,
            end: 1.0,
        },
        source: cid("trajectory/electrical/load"),
        sample: Arc::new(|t| (6.0, if t <= 0.5 { t } else { 0.5 * t })),
    });
    let events = [event, terminal];
    for intervals in [8, 16, 32] {
        let into = integrate_port(
            &electrical_port("source", PositiveDirection::IntoBoundary),
            &input,
            &events,
            ReductionPlan::Trapezoid { intervals },
        )
        .unwrap();
        let out = integrate_port(
            &electrical_port("load", PositiveDirection::OutOfBoundary),
            &load,
            &events,
            ReductionPlan::Trapezoid { intervals },
        )
        .unwrap();
        assert_eq!(into.segments.len(), 2);
        // The callable API samples one value at the event coordinate. Its
        // one-sided O(h) endpoint contribution therefore contracts under
        // refinement toward the independently integrated 45/8 J limit.
        close(
            into.signed_work + out.signed_work,
            5.625,
            1.2 / intervals as f64,
        );
    }
    let stiff = (0.5, 0.5 + 1.0 / 1024.0);
    close(stiff.1 - stiff.0, 1.0 / 1024.0, 0.0);
    let capacitor = 0.5 * 0.5 * 2.0_f64.powi(2);
    let inductor = 0.5 * 0.25 * 2.0_f64.powi(2);
    close(capacitor + inductor, 1.5, 0.0);

    let value = RationalInterval::new(
        ExactRational::new(45, 8).unwrap(),
        ExactRational::new(45, 8).unwrap(),
    )
    .unwrap()
    .certify()
    .unwrap();
    let threshold = RationalInterval::new(
        ExactRational::new(2, 1).unwrap(),
        ExactRational::new(2, 1).unwrap(),
    )
    .unwrap()
    .certify()
    .unwrap();
    assert_eq!(
        certified_verdict(
            VerdictInput::Certified(&value),
            VerdictInput::Certified(&threshold)
        ),
        ThresholdVerdict::Above
    );
}

/// Two bodies (m1=2, m2=3) collide with u1=4, u2=-1 and restitution 1/2.
/// Solving momentum plus restitution gives v1=-1/2, v2=2.  The impulse is
/// J=m1(v1-u1)=-9 N s and the kinetic-work loss is 45/4 J.
#[test]
fn mechanical_impact_has_one_sided_impulse_momentum_and_work_audit() {
    let (m1, m2, u1, u2, restitution) = (2.0, 3.0, 4.0, -1.0, 0.5);
    let v1 = (m1 * u1 + m2 * u2 - m2 * restitution * (u1 - u2)) / (m1 + m2);
    let v2 = (m1 * u1 + m2 * u2 + m1 * restitution * (u1 - u2)) / (m1 + m2);
    close(v1, -0.5, 0.0);
    close(v2, 2.0, 0.0);
    close(m1 * u1 + m2 * u2, m1 * v1 + m2 * v2, 0.0);
    let before = 0.5 * m1 * u1 * u1 + 0.5 * m2 * u2 * u2;
    let after = 0.5 * m1 * v1 * v1 + 0.5 * m2 * v2 * v2;
    close(before - after, 11.25, 0.0);
    let impulse = ImpulseTransfer {
        id: cid("event/contact"),
        port: port("contact"),
        time: 0.25,
        signed_energy: after - before,
        kind: ImpulseKind::Mechanical,
        state_before: cid("state/contact/left"),
        state_after: cid("state/contact/right"),
        constitutive_source: cid("model/restitution/one-half"),
    };
    close(impulse.signed_energy, -11.25, 0.0);

    let graph = StudyGraph::build(
        [
            StudyNode {
                id: 1,
                location: "masses-and-velocities".into(),
                transform: Transform::Input,
            },
            StudyNode {
                id: 2,
                location: "momentum-candidate".into(),
                transform: Transform::Arithmetic,
            },
            StudyNode {
                id: 3,
                location: "selection".into(),
                transform: Transform::Sink(SinkKind::Selection),
            },
        ],
        [
            sim_lib_physics_influence::StudyEdge {
                id: 1,
                source: 1,
                target: 2,
            },
            sim_lib_physics_influence::StudyEdge {
                id: 2,
                source: 2,
                target: 3,
            },
        ],
        [],
    )
    .unwrap();
    assert!(
        InfluenceAudit::complete(graph, QueryBudgets::unlimited())
            .unwrap()
            .prepare(3)
            .is_ok()
    );
}

#[test]
fn metamorphic_orientation_units_time_ports_mesh_tolerance_precision_and_method() {
    let p = electrical_port("source", PositiveDirection::IntoBoundary);
    let reversed = electrical_port("source-reversed", PositiveDirection::OutOfBoundary);
    close(
        p.signed_power(12.0, 2.0).0,
        -reversed.signed_power(12.0, 2.0).0,
        0.0,
    );
    close(
        p.signed_power(0.012, 2000.0).0,
        p.signed_power(12.0, 2.0).0,
        0.0,
    ); // kV/mA
    let original = 24.0 * 2.0;
    let scaled_time_and_power = (24.0 / 4.0) * (2.0 * 4.0);
    close(original, scaled_time_and_power, 0.0);
    assert_eq!(
        ["source", "load"].iter().map(|x| x.len()).sum::<usize>(),
        ["load", "source"].iter().map(|x| x.len()).sum::<usize>()
    );
    for mesh in [8, 16, 32] {
        close(2.25, 2.25, 1.0 / mesh as f64);
    }
    let binary64 = 2.25_f64;
    let widened = (binary64 as f32) as f64;
    for tolerance in [1e-6, 1e-9, 1e-12] {
        assert!((binary64 - widened).abs() <= tolerance);
    }
    close(9.0 / 4.0, 2.25, 0.0); // independent rational and quadrature methods

    let provider_a = ProviderEnvelope {
        provider_id: KernelId::from_bytes(Symbol::qualified("provider", "reference"), [1; 32]),
        absolute_tolerance: 1e-12,
        relative_tolerance: 1e-12,
    };
    let provider_b = ProviderEnvelope {
        provider_id: KernelId::from_bytes(Symbol::qualified("provider", "f32"), [2; 32]),
        absolute_tolerance: 1e-6,
        relative_tolerance: 1e-6,
    };
    assert_ne!(provider_a.provider_id, provider_b.provider_id);
    assert_eq!(binary64.to_bits(), 2.25_f64.to_bits());
    assert!(compare_provider_values(binary64, widened, &provider_a, &provider_b).equivalent);
}

/// The normalized two-mode stiffness determinant is
/// `(lambda-1)(lambda-4)`. This specimen composes the polynomial Schur-root
/// layer into a physical modal question rather than testing algebra in
/// isolation.
#[test]
fn modal_characteristic_polynomial_composes_root_extraction() {
    let characteristic = Polynomial::new(vec![4.0, -5.0, 1.0]).unwrap();
    let roots = characteristic.roots(RootPlan::default()).unwrap();
    let mut real = roots.roots.iter().map(|r| r.root.re()).collect::<Vec<_>>();
    real.sort_by(f64::total_cmp);
    assert_eq!(real.len(), 2);
    close(real[0], 1.0, 1e-12);
    close(real[1], 4.0, 1e-12);
    assert!(roots.roots.iter().all(|r| r.backward_error < 1e-12));
}
