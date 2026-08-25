use sim_lib_physics_power::*;
use std::sync::Arc;

fn main() {
    let boundary = BoundaryId::new("motor").unwrap();
    let event = WorkEvent {
        id: ContentId::new("event/switch").unwrap(),
        time: 1.0,
        sides: None,
        terminal: false,
    };
    for (id, pair, direction, values) in [
        (
            "electrical",
            PowerPair::voltage_current(),
            PositiveDirection::IntoBoundary,
            (12.0, 2.0),
        ),
        (
            "shaft",
            PowerPair::torque_angular_velocity(),
            PositiveDirection::OutOfBoundary,
            (3.0, 8.0),
        ),
    ] {
        let port = ConjugatePort {
            boundary: boundary.clone(),
            port: PortRef::new(id).unwrap(),
            pair,
            positive: direction,
        };
        let history = PortHistory::Callable(CallableHistory {
            span: WorkSpan {
                start: 0.0,
                end: 2.0,
            },
            source: ContentId::new(format!("trajectory/{id}")).unwrap(),
            sample: Arc::new(move |_| values),
        });
        let work = integrate_port(
            &port,
            &history,
            std::slice::from_ref(&event),
            ReductionPlan::Trapezoid { intervals: 8 },
        )
        .unwrap();
        println!(
            "{id}: {} J in {} segments",
            work.signed_work,
            work.segments.len()
        );
    }
}
