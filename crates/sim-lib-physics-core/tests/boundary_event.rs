use sim_lib_numbers_quantity::{BaseDimension, Dimension, ExactScalar, MeasureRole, Quantity};
// conformance: immutable boundary and event graph contract
use sim_lib_physics_core::*;
fn q(v: i64) -> PhysicalQuantity {
    Quantity::new(
        ExactScalar::from(v),
        Dimension::base(BaseDimension::Time),
        None,
        None,
        MeasureRole::Interval,
    )
    .unwrap()
}
fn graph() -> EventGraph {
    let b = Boundary {
        id: BoundaryId::new("tank").unwrap(),
        closure: BoundaryClosure::Open,
        stores: vec![StoreRef::new("thermal").unwrap()],
        ports: vec![PortRef::new("heater").unwrap()],
    };
    let e = Event {
        id: EventRef::new("switch-on").unwrap(),
        at: q(1),
        influences: vec![StateRef::new("temperature").unwrap()],
    };
    let t = Transfer {
        from: Endpoint::SpanStart,
        to: Endpoint::Event(e.id.clone()),
        path: TransferPath::Crossing {
            boundary: b.id.clone(),
            port: b.ports[0].clone(),
            store: b.stores[0].clone(),
        },
        signed_power: q(10),
    };
    let mut g = EventGraph {
        id: String::new(),
        span: TimeSpan {
            start: q(0),
            end: q(2),
        },
        boundaries: vec![b],
        events: vec![e],
        transfers: vec![t],
    };
    g.id = g.canonical_id();
    g
}
#[test]
fn valid_graph_round_trips() {
    let g = graph();
    g.validate().unwrap();
    assert!(g.read_construct().starts_with("#(physics/EventGraph"));
    assert_eq!(SHAPES.len(), 4)
}
#[test]
fn simultaneous_events_are_legal() {
    let mut g = graph();
    let e = Event {
        id: EventRef::new("sample").unwrap(),
        at: q(1),
        influences: vec![],
    };
    g.transfers.push(Transfer {
        from: Endpoint::Event(g.events[0].id.clone()),
        to: Endpoint::Event(e.id.clone()),
        path: TransferPath::Internal {
            boundary: g.boundaries[0].id.clone(),
            from: g.boundaries[0].stores[0].clone(),
            to: g.boundaries[0].stores[0].clone(),
        },
        signed_power: q(0),
    });
    g.events.push(e);
    g.id = g.canonical_id();
    g.validate().unwrap()
}
#[test]
fn rejects_bad_topology() {
    let mut g = graph();
    if let TransferPath::Crossing { port, .. } = &mut g.transfers[0].path {
        *port = PortRef::new("missing").unwrap()
    }
    g.id = g.canonical_id();
    assert!(matches!(
        g.validate(),
        Err(PhysicsError::InvalidTransfer(_))
    ));
    let mut g = graph();
    g.boundaries[0].closure = BoundaryClosure::Closed;
    g.id = g.canonical_id();
    assert!(matches!(
        g.validate(),
        Err(PhysicsError::InvalidTransfer(_))
    ));
    let mut g = graph();
    g.events.push(Event {
        id: EventRef::new("orphan").unwrap(),
        at: q(2),
        influences: vec![],
    });
    g.id = g.canonical_id();
    assert_eq!(
        g.validate(),
        Err(PhysicsError::Unreachable("orphan".into()))
    )
}
#[test]
fn rejects_duplicates_order_and_tampering() {
    let mut g = graph();
    let duplicate_store = g.boundaries[0].stores[0].clone();
    g.boundaries[0].stores.push(duplicate_store);
    g.id = g.canonical_id();
    assert!(matches!(
        g.validate(),
        Err(PhysicsError::Duplicate("store", _))
    ));
    let mut g = graph();
    g.events[0].at = q(3);
    g.id = g.canonical_id();
    assert_eq!(g.validate(), Err(PhysicsError::EndpointOrder));
    let mut g = graph();
    g.id = "wrong".into();
    assert!(matches!(
        g.validate(),
        Err(PhysicsError::IdentityMismatch { .. })
    ))
}
