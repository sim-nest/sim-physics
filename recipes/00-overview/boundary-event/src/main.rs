use sim_lib_numbers_quantity::{BaseDimension, Dimension, ExactScalar, MeasureRole, Quantity};
use sim_lib_physics_core::*;
fn main() {
    let t = |n| {
        Quantity::new(
            ExactScalar::from(n),
            Dimension::base(BaseDimension::Time),
            None,
            None,
            MeasureRole::Interval,
        )
        .unwrap()
    };
    let mut g = EventGraph {
        id: String::new(),
        span: TimeSpan {
            start: t(0),
            end: t(1),
        },
        boundaries: vec![Boundary {
            id: BoundaryId::new("insulated-vessel").unwrap(),
            closure: BoundaryClosure::Closed,
            stores: vec![StoreRef::new("thermal").unwrap()],
            ports: vec![],
        }],
        events: vec![],
        transfers: vec![],
    };
    g.id = g.canonical_id();
    g.validate().unwrap();
    println!("{}", g.read_construct())
}
