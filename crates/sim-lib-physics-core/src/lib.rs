#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Explicit, immutable topology for lumped physical studies.
//!
//! Validation is deliberately solve-independent: a solver never receives an
//! ambiguous boundary, undeclared crossing, dangling endpoint, or unordered
//! event graph.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

pub use sim_lib_numbers_quantity::{ExactScalar, Quantity};

/// Semantic physical value used throughout the boundary graph.
pub type PhysicalQuantity = Quantity<ExactScalar>;

macro_rules! stable_id {
    ($name:ident) => {
        #[doc = concat!("Stable identifier for a `", stringify!($name), "` record.")]
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
        pub struct $name(String);
        impl $name {
            /// Creates an identifier containing lowercase ASCII path segments.
            pub fn new(value: impl Into<String>) -> Result<Self, PhysicsError> {
                let value = value.into();
                if value.is_empty()
                    || !value.bytes().all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || matches!(b, b'-' | b'/' | b'_')
                    })
                {
                    return Err(PhysicsError::InvalidId(value));
                }
                Ok(Self(value))
            }
            /// Returns the stable textual representation.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

stable_id!(BoundaryId);
stable_id!(StateRef);
stable_id!(StoreRef);
stable_id!(PortRef);
stable_id!(EventRef);

/// Closed interval with semantic time endpoints.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeSpan {
    /// Inclusive start time.
    pub start: PhysicalQuantity,
    /// Inclusive end time.
    pub end: PhysicalQuantity,
}

/// Explicit declaration of whether a boundary exchanges power with its exterior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryClosure {
    /// No crossing is legal; this is never inferred from an empty port list.
    Closed,
    /// Crossings are legal only through declared ports.
    Open,
}

/// Lumped-system boundary and its declared members.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Boundary {
    /// Stable boundary identity.
    pub id: BoundaryId,
    /// Explicit closure declaration.
    pub closure: BoundaryClosure,
    /// Stores internal to this boundary.
    pub stores: Vec<StoreRef>,
    /// Ports through which signed power may cross.
    pub ports: Vec<PortRef>,
}

/// Endpoint of an event-graph transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Endpoint {
    /// A declared event.
    Event(EventRef),
    /// The declared start of the study span.
    SpanStart,
    /// The declared end of the study span.
    SpanEnd,
}

/// Topological location of a transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferPath {
    /// Transfer between two stores within one boundary.
    Internal {
        /// Owning boundary.
        boundary: BoundaryId,
        /// Source store.
        from: StoreRef,
        /// Destination store.
        to: StoreRef,
    },
    /// Transfer crossing a boundary through one declared port.
    Crossing {
        /// Boundary being crossed.
        boundary: BoundaryId,
        /// Declared crossing port.
        port: PortRef,
        /// Store on the internal side.
        store: StoreRef,
    },
}

/// Immutable transfer between two declared endpoints.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    /// Source endpoint.
    pub from: Endpoint,
    /// Destination endpoint.
    pub to: Endpoint,
    /// Internal or boundary-crossing topology.
    pub path: TransferPath,
    /// Signed physical transfer; sign is interpreted relative to `path`.
    pub signed_power: PhysicalQuantity,
}

/// One event in total event order. Equal times model simultaneous events.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// Stable event identity.
    pub id: EventRef,
    /// Semantic event time.
    pub at: PhysicalQuantity,
    /// States influenced by this event.
    pub influences: Vec<StateRef>,
}

/// Complete immutable graph accepted by downstream studies and solvers.
#[derive(Clone, Debug, PartialEq)]
pub struct EventGraph {
    /// Stable graph identity.
    pub id: String,
    /// Declared study span.
    pub span: TimeSpan,
    /// Boundary declarations.
    pub boundaries: Vec<Boundary>,
    /// Events in nondecreasing time order.
    pub events: Vec<Event>,
    /// Transfers joining events or span endpoints.
    pub transfers: Vec<Transfer>,
}

/// Validation refusal emitted before a solve can run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhysicsError {
    /// A stable identifier violates its lexical contract.
    InvalidId(String),
    /// A record identity is duplicated.
    Duplicate(&'static str, String),
    /// A referenced record is absent.
    Missing(&'static str, String),
    /// Time quantities cannot be compared or are out of order.
    EndpointOrder,
    /// A transfer contradicts boundary topology or closure.
    InvalidTransfer(String),
    /// An event is not connected to the declared span.
    Unreachable(String),
    /// Stored graph identity differs from canonical data identity.
    IdentityMismatch {
        /// Identity computed from canonical data.
        expected: String,
        /// Identity carried by the graph.
        actual: String,
    },
}

impl fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "invalid stable id `{id}`"),
            Self::Duplicate(kind, id) => write!(f, "duplicate {kind} `{id}`"),
            Self::Missing(kind, id) => write!(f, "missing {kind} `{id}`"),
            Self::EndpointOrder => f.write_str("event/span endpoints are not ordered"),
            Self::InvalidTransfer(reason) => write!(f, "invalid transfer: {reason}"),
            Self::Unreachable(id) => write!(f, "event `{id}` is unreachable from span start"),
            Self::IdentityMismatch { expected, actual } => write!(
                f,
                "graph identity mismatch: expected `{expected}`, got `{actual}`"
            ),
        }
    }
}
impl std::error::Error for PhysicsError {}

impl EventGraph {
    /// Validates identity, topology, ordering, reachability, and references.
    pub fn validate(&self) -> Result<(), PhysicsError> {
        let expected = self.canonical_id();
        if self.id != expected {
            return Err(PhysicsError::IdentityMismatch {
                expected,
                actual: self.id.clone(),
            });
        }
        if quantity_cmp(&self.span.start, &self.span.end)? == std::cmp::Ordering::Greater {
            return Err(PhysicsError::EndpointOrder);
        }

        let mut boundaries = BTreeMap::new();
        let mut stores = BTreeSet::new();
        let mut ports = BTreeSet::new();
        for boundary in &self.boundaries {
            if boundaries.insert(boundary.id.clone(), boundary).is_some() {
                return Err(PhysicsError::Duplicate(
                    "boundary",
                    boundary.id.as_str().into(),
                ));
            }
            for store in &boundary.stores {
                if !stores.insert(store.clone()) {
                    return Err(PhysicsError::Duplicate("store", store.as_str().into()));
                }
            }
            for port in &boundary.ports {
                if !ports.insert(port.clone()) {
                    return Err(PhysicsError::Duplicate("port", port.as_str().into()));
                }
            }
            if boundary.closure == BoundaryClosure::Closed && !boundary.ports.is_empty() {
                return Err(PhysicsError::InvalidTransfer(format!(
                    "closed boundary `{}` declares ports",
                    boundary.id.as_str()
                )));
            }
        }

        let mut events = BTreeMap::new();
        let mut event_ranks = BTreeMap::new();
        let mut previous = &self.span.start;
        for (rank, event) in self.events.iter().enumerate() {
            if events.insert(event.id.clone(), event).is_some() {
                return Err(PhysicsError::Duplicate("event", event.id.as_str().into()));
            }
            if quantity_cmp(previous, &event.at)? == std::cmp::Ordering::Greater
                || quantity_cmp(&event.at, &self.span.end)? == std::cmp::Ordering::Greater
            {
                return Err(PhysicsError::EndpointOrder);
            }
            previous = &event.at;
            event_ranks.insert(event.id.clone(), rank + 1);
        }

        let mut adjacency: BTreeMap<Option<EventRef>, Vec<EventRef>> = BTreeMap::new();
        for transfer in &self.transfers {
            check_endpoint(&transfer.from, &events)?;
            check_endpoint(&transfer.to, &events)?;
            if endpoint_rank(&transfer.from, &event_ranks)
                > endpoint_rank(&transfer.to, &event_ranks)
            {
                return Err(PhysicsError::EndpointOrder);
            }
            match &transfer.path {
                TransferPath::Internal { boundary, from, to } => {
                    let b = boundaries.get(boundary).ok_or_else(|| {
                        PhysicsError::Missing("boundary", boundary.as_str().into())
                    })?;
                    if !b.stores.contains(from) || !b.stores.contains(to) {
                        return Err(PhysicsError::InvalidTransfer(
                            "internal transfer must join stores in one boundary".into(),
                        ));
                    }
                }
                TransferPath::Crossing {
                    boundary,
                    port,
                    store,
                } => {
                    let b = boundaries.get(boundary).ok_or_else(|| {
                        PhysicsError::Missing("boundary", boundary.as_str().into())
                    })?;
                    if b.closure != BoundaryClosure::Open
                        || !b.ports.contains(port)
                        || !b.stores.contains(store)
                    {
                        return Err(PhysicsError::InvalidTransfer(
                            "crossing must use a declared port and store on an open boundary"
                                .into(),
                        ));
                    }
                }
            }
            if let Endpoint::Event(to) = &transfer.to {
                let from = match &transfer.from {
                    Endpoint::SpanStart => None,
                    Endpoint::Event(id) => Some(id.clone()),
                    Endpoint::SpanEnd => return Err(PhysicsError::EndpointOrder),
                };
                adjacency.entry(from).or_default().push(to.clone());
            }
        }
        let mut reached = BTreeSet::new();
        let mut queue: VecDeque<EventRef> = adjacency
            .get(&None)
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        while let Some(id) = queue.pop_front() {
            if reached.insert(id.clone()) {
                queue.extend(adjacency.get(&Some(id)).into_iter().flatten().cloned());
            }
        }
        for id in events.keys() {
            if !reached.contains(id) {
                return Err(PhysicsError::Unreachable(id.as_str().into()));
            }
        }
        Ok(())
    }

    /// Computes deterministic canonical data identity over all immutable fields except `id`.
    pub fn canonical_id(&self) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in format!(
            "{:?}{:?}{:?}{:?}",
            self.span, self.boundaries, self.events, self.transfers
        )
        .bytes()
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("physics/event-graph/{hash:016x}")
    }

    /// Emits the preferred read-construct representation.
    pub fn read_construct(&self) -> String {
        format!("#(physics/EventGraph \"{}\")", self.id)
    }
}

fn quantity_cmp(
    a: &PhysicalQuantity,
    b: &PhysicalQuantity,
) -> Result<std::cmp::Ordering, PhysicsError> {
    if a.dimension() != b.dimension() || a.kind() != b.kind() || a.role() != b.role() {
        return Err(PhysicsError::EndpointOrder);
    }
    let left = a
        .scalar()
        .numerator()
        .checked_mul(b.scalar().denominator())
        .ok_or(PhysicsError::EndpointOrder)?;
    let right = b
        .scalar()
        .numerator()
        .checked_mul(a.scalar().denominator())
        .ok_or(PhysicsError::EndpointOrder)?;
    Ok(left.cmp(&right))
}
fn check_endpoint(
    endpoint: &Endpoint,
    events: &BTreeMap<EventRef, &Event>,
) -> Result<(), PhysicsError> {
    if let Endpoint::Event(id) = endpoint
        && !events.contains_key(id)
    {
        return Err(PhysicsError::Missing("event", id.as_str().into()));
    }
    Ok(())
}
fn endpoint_rank(endpoint: &Endpoint, ranks: &BTreeMap<EventRef, usize>) -> usize {
    match endpoint {
        Endpoint::SpanStart => 0,
        Endpoint::Event(id) => ranks.get(id).copied().unwrap_or(usize::MAX - 1),
        Endpoint::SpanEnd => usize::MAX,
    }
}

/// Public Shape descriptions installed by the physics core library.
pub const SHAPES: &[&str] = &[
    "physics/TimeSpan",
    "physics/Boundary",
    "physics/Event",
    "physics/EventGraph",
];
