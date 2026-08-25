use sim_lib_physics_power::*;
use std::sync::Arc;

fn port(pair: PowerPair, positive: PositiveDirection, id: &str) -> ConjugatePort {
    ConjugatePort {
        boundary: BoundaryId::new("system").unwrap(),
        port: PortRef::new(id).unwrap(),
        pair,
        positive,
    }
}
fn callable(
    source: &str,
    end: f64,
    f: impl Fn(f64) -> (f64, f64) + Send + Sync + 'static,
) -> PortHistory {
    PortHistory::Callable(CallableHistory {
        span: WorkSpan { start: 0.0, end },
        source: ContentId::new(source).unwrap(),
        sample: Arc::new(f),
    })
}
fn integrate(p: &ConjugatePort, h: &PortHistory, events: &[WorkEvent]) -> PortWork {
    integrate_port(p, h, events, ReductionPlan::Trapezoid { intervals: 100 }).unwrap()
}

#[test]
fn analytic_electrical_translational_and_rotational_ports() {
    for pair in [
        PowerPair::voltage_current(),
        PowerPair::force_velocity(),
        PowerPair::torque_angular_velocity(),
    ] {
        let work = integrate(
            &port(pair, PositiveDirection::IntoBoundary, "in"),
            &callable("trajectory/linear", 2.0, |t| (t, 3.0)),
            &[],
        );
        assert!((work.signed_work - 6.0).abs() < 1e-12);
        assert_eq!(work.segments.len(), 1);
    }
    assert_eq!(
        PowerPair::pressure_volume_flow().effort_shape,
        "quantity/pressure"
    );
}

#[test]
fn orientation_and_zero_power_are_explicit() {
    let incoming = port(
        PowerPair::voltage_current(),
        PositiveDirection::IntoBoundary,
        "wire",
    );
    let outgoing = port(
        PowerPair::voltage_current(),
        PositiveDirection::OutOfBoundary,
        "wire",
    );
    let history = callable("trajectory/constant", 4.0, |_| (2.0, 5.0));
    assert!((integrate(&incoming, &history, &[]).signed_work - 40.0).abs() < 1e-12);
    assert!((integrate(&outgoing, &history, &[]).signed_work + 40.0).abs() < 1e-12);
    assert_eq!(
        integrate(
            &incoming,
            &callable("trajectory/zero", 4.0, |_| (0.0, 99.0)),
            &[]
        )
        .signed_work,
        0.0
    );
}

#[test]
fn event_split_rejects_a_wrong_mesh_sample_without_sides() {
    let p = port(
        PowerPair::force_velocity(),
        PositiveDirection::IntoBoundary,
        "shaft",
    );
    let history = PortHistory::Sampled(SampledHistory {
        span: WorkSpan {
            start: 0.0,
            end: 2.0,
        },
        source: ContentId::new("trajectory/switched").unwrap(),
        samples: vec![
            PortSample {
                time: 0.0,
                effort: 1.0,
                flow: 1.0,
            },
            PortSample {
                time: 1.0,
                effort: 1.0,
                flow: 1.0,
            },
            PortSample {
                time: 1.0,
                effort: 99.0,
                flow: 1.0,
            },
            PortSample {
                time: 2.0,
                effort: 2.0,
                flow: 1.0,
            },
        ],
    });
    let mut event = WorkEvent {
        id: ContentId::new("event/switch").unwrap(),
        time: 1.0,
        sides: None,
        terminal: false,
    };
    assert!(matches!(
        integrate_port(
            &p,
            &history,
            &[event.clone()],
            ReductionPlan::SampledTrapezoid
        ),
        Err(PowerError::MissingOneSidedValues(_))
    ));
    event.sides = Some(OneSidedValues {
        left: (1.0, 1.0),
        right: (2.0, 1.0),
    });
    // Duplicate mesh values still cannot masquerade as one canonical endpoint sample.
    assert!(integrate_port(&p, &history, &[event], ReductionPlan::SampledTrapezoid).is_err());
}

#[test]
fn simultaneous_events_split_once_and_terminal_is_endpoint() {
    let p = port(
        PowerPair::voltage_current(),
        PositiveDirection::IntoBoundary,
        "wire",
    );
    let h = callable("trajectory/events", 2.0, |_| (1.0, 1.0));
    let events = vec![
        WorkEvent {
            id: ContentId::new("event/a").unwrap(),
            time: 1.0,
            sides: None,
            terminal: false,
        },
        WorkEvent {
            id: ContentId::new("event/b").unwrap(),
            time: 1.0,
            sides: None,
            terminal: false,
        },
        WorkEvent {
            id: ContentId::new("event/end").unwrap(),
            time: 2.0,
            sides: None,
            terminal: true,
        },
    ];
    let work = integrate(&p, &h, &events);
    assert_eq!(work.segments.len(), 2);
    assert_eq!(work.segments.last().unwrap().end, 2.0);
    let bad = [WorkEvent {
        id: ContentId::new("event/early").unwrap(),
        time: 1.0,
        sides: None,
        terminal: true,
    }];
    assert_eq!(
        integrate_port(&p, &h, &bad, ReductionPlan::Trapezoid { intervals: 4 }),
        Err(PowerError::TerminalEndpointMismatch)
    );
}

#[test]
fn pure_impulse_is_separate_from_zero_continuous_work() {
    let p = port(
        PowerPair::force_velocity(),
        PositiveDirection::IntoBoundary,
        "impact",
    );
    let work = integrate(&p, &callable("trajectory/rest", 1.0, |_| (5.0, 0.0)), &[]);
    let impulse = ImpulseTransfer {
        id: ContentId::new("event/impact").unwrap(),
        port: p.port.clone(),
        time: 1.0,
        signed_energy: 12.0,
        kind: ImpulseKind::Mechanical,
        state_before: ContentId::new("state/before").unwrap(),
        state_after: ContentId::new("state/after").unwrap(),
        constitutive_source: ContentId::new("law/restitution").unwrap(),
    };
    assert_eq!(work.signed_work, 0.0);
    assert_eq!(impulse.signed_energy, 12.0);
}
