#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Boundary-relative conjugate ports and event-split signed work.

use std::fmt;
use std::sync::Arc;

use sim_lib_numbers_quantity::{BaseDimension, Dimension, Exponent};
pub use sim_lib_physics_core::{BoundaryId, PortRef};

/// Stable content identity supplied by the owner of a trajectory or event.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ContentId(String);

impl ContentId {
    /// Creates a non-empty, path-like content identity.
    pub fn new(value: impl Into<String>) -> Result<Self, PowerError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/' | b':' | b'.'))
        {
            return Err(PowerError::InvalidIdentity(value));
        }
        Ok(Self(value))
    }

    /// Returns canonical identity text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Open semantic description of one effort/flow pairing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PowerPair {
    /// Stable semantic pairing identity.
    pub id: ContentId,
    /// Shape admitted for the effort quantity.
    pub effort_shape: String,
    /// Shape admitted for the flow quantity.
    pub flow_shape: String,
    /// Exact effort dimension.
    pub effort_dimension: Dimension,
    /// Exact flow dimension.
    pub flow_dimension: Dimension,
}

impl PowerPair {
    /// Defines an open user pairing, proving that its dimensions multiply to power.
    pub fn new(
        id: ContentId,
        effort_shape: impl Into<String>,
        flow_shape: impl Into<String>,
        effort_dimension: Dimension,
        flow_dimension: Dimension,
    ) -> Result<Self, PowerError> {
        let pair = Self {
            id,
            effort_shape: effort_shape.into(),
            flow_shape: flow_shape.into(),
            effort_dimension,
            flow_dimension,
        };
        if pair.effort_shape.is_empty()
            || pair.flow_shape.is_empty()
            || pair
                .effort_dimension
                .product(&pair.flow_dimension)
                .map_err(|_| PowerError::NotPowerPair)?
                != power_dimension()
        {
            return Err(PowerError::NotPowerPair);
        }
        Ok(pair)
    }

    /// Electrical voltage/current pairing.
    pub fn voltage_current() -> Self {
        builtin(
            "voltage-current",
            "quantity/voltage",
            "quantity/current",
            voltage_dimension(),
            current_dimension(),
        )
    }
    /// Translational force/velocity pairing.
    pub fn force_velocity() -> Self {
        builtin(
            "force-velocity",
            "quantity/force",
            "quantity/velocity",
            force_dimension(),
            velocity_dimension(),
        )
    }
    /// Rotational torque/angular-velocity pairing.
    pub fn torque_angular_velocity() -> Self {
        builtin(
            "torque-angular-velocity",
            "quantity/torque",
            "quantity/angular-velocity",
            torque_dimension(),
            angular_velocity_dimension(),
        )
    }
    /// Fluid pressure/volume-flow pairing.
    pub fn pressure_volume_flow() -> Self {
        builtin(
            "pressure-volume-flow",
            "quantity/pressure",
            "quantity/volume-flow",
            pressure_dimension(),
            volume_flow_dimension(),
        )
    }
}

/// Meaning of positive signed power at one boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositiveDirection {
    /// Positive power enters the bounded system.
    IntoBoundary,
    /// Positive power leaves the bounded system.
    OutOfBoundary,
}

impl PositiveDirection {
    fn factor(self) -> f64 {
        if self == Self::IntoBoundary {
            1.0
        } else {
            -1.0
        }
    }
}

/// Immutable declaration of a boundary port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConjugatePort {
    /// Core boundary identity.
    pub boundary: BoundaryId,
    /// Core port identity.
    pub port: PortRef,
    /// Semantic effort/flow pairing.
    pub pair: PowerPair,
    /// Once-declared positive direction.
    pub positive: PositiveDirection,
}

impl ConjugatePort {
    /// Computes boundary-relative instantaneous signed power.
    pub fn signed_power(&self, effort: f64, flow: f64) -> SignedPower {
        SignedPower(effort * flow * self.positive.factor())
    }
}

/// Instantaneous signed power in canonical watts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignedPower(pub f64);

/// Closed time span, in canonical seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkSpan {
    /// Inclusive start.
    pub start: f64,
    /// Inclusive end.
    pub end: f64,
}

/// One sampled effort/flow value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortSample {
    /// Sample time.
    pub time: f64,
    /// Effort value.
    pub effort: f64,
    /// Flow value.
    pub flow: f64,
}

/// Explicit one-sided values at a discontinuity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OneSidedValues {
    /// Left-limit effort/flow.
    pub left: (f64, f64),
    /// Right-limit effort/flow.
    pub right: (f64, f64),
}

/// Declared split in a continuous history.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkEvent {
    /// Stable event identity.
    pub id: ContentId,
    /// Exact event time.
    pub time: f64,
    /// Required limits when the port jumps.
    pub sides: Option<OneSidedValues>,
    /// Whether integration must terminate here.
    pub terminal: bool,
}

/// Callable effort/flow history. The closure is an adapter to the canonical callable owner.
#[derive(Clone)]
pub struct CallableHistory {
    /// Explicit source span.
    pub span: WorkSpan,
    /// Source trajectory/content identity.
    pub source: ContentId,
    /// Callable sample adapter.
    pub sample: Arc<dyn Fn(f64) -> (f64, f64) + Send + Sync>,
}

/// Sampled effort/flow history.
#[derive(Clone, Debug, PartialEq)]
pub struct SampledHistory {
    /// Explicit source span.
    pub span: WorkSpan,
    /// Source trajectory/content identity.
    pub source: ContentId,
    /// Ordered samples including span endpoints.
    pub samples: Vec<PortSample>,
}

/// Continuous source accepted by the work audit.
#[derive(Clone)]
pub enum PortHistory {
    /// Runtime callable/ODE dense path adapter.
    Callable(CallableHistory),
    /// Sampled trajectory.
    Sampled(SampledHistory),
}

/// Explicit quadrature/reduction policy.
#[derive(Clone, Debug, PartialEq)]
pub enum ReductionPlan {
    /// Composite trapezoid with a fixed interval count.
    Trapezoid {
        /// Number of intervals per event segment.
        intervals: usize,
    },
    /// Consume the declared sampled mesh exactly.
    SampledTrapezoid,
}

/// Inspectable evidence for one continuous segment.
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentEvidence {
    /// Segment start.
    pub start: f64,
    /// Segment end.
    pub end: f64,
    /// Method name.
    pub method: String,
    /// Number of integrand evaluations.
    pub evaluations: usize,
    /// Signed segment work in joules.
    pub signed_work: f64,
    /// Conservative local uncertainty estimate.
    pub uncertainty: f64,
}

/// Signed continuous work for exactly one port.
#[derive(Clone, Debug, PartialEq)]
pub struct PortWork {
    /// Boundary and port declaration.
    pub port: ConjugatePort,
    /// Source content identity.
    pub source: ContentId,
    /// Exact audited span.
    pub span: WorkSpan,
    /// Explicit reduction plan.
    pub plan: ReductionPlan,
    /// Per-event segments.
    pub segments: Vec<SegmentEvidence>,
    /// Signed continuous work; impulses excluded.
    pub signed_work: f64,
    /// Sum of segment uncertainty bounds.
    pub uncertainty: f64,
}

/// Kind of an instantaneous transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImpulseKind {
    /// Mechanical impulse.
    Mechanical,
    /// Electrical switching transfer.
    Electrical,
    /// Open user-defined impulse admitted by Shape.
    User(String),
}

/// Event transfer kept separate from continuous work.
#[derive(Clone, Debug, PartialEq)]
pub struct ImpulseTransfer {
    /// Event content identity.
    pub id: ContentId,
    /// Affected port.
    pub port: PortRef,
    /// Event time.
    pub time: f64,
    /// Signed transferred energy in joules.
    pub signed_energy: f64,
    /// Semantic impulse kind.
    pub kind: ImpulseKind,
    /// Canonical state before the event.
    pub state_before: ContentId,
    /// Canonical state after the event.
    pub state_after: ContentId,
    /// Constitutive source identity.
    pub constitutive_source: ContentId,
}

/// Refusal from a power/work audit.
#[derive(Clone, Debug, PartialEq)]
pub enum PowerError {
    /// Invalid stable identity.
    InvalidIdentity(String),
    /// Dimensions do not multiply to power.
    NotPowerPair,
    /// Span, event, or samples are malformed.
    InvalidHistory(String),
    /// A jumping port omitted one-sided values.
    MissingOneSidedValues(String),
    /// Terminal event does not equal integration endpoint.
    TerminalEndpointMismatch,
}

impl fmt::Display for PowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PowerError {}

/// Integrates one port without summing it with any other port or event impulse.
pub fn integrate_port(
    port: &ConjugatePort,
    history: &PortHistory,
    events: &[WorkEvent],
    plan: ReductionPlan,
) -> Result<PortWork, PowerError> {
    let (span, source) = match history {
        PortHistory::Callable(h) => (h.span, h.source.clone()),
        PortHistory::Sampled(h) => (h.span, h.source.clone()),
    };
    if !span.start.is_finite() || !span.end.is_finite() || span.start > span.end {
        return Err(PowerError::InvalidHistory(
            "span is not finite and ordered".into(),
        ));
    }
    let mut splits = vec![span.start];
    let mut previous = span.start;
    for event in events {
        if !event.time.is_finite()
            || event.time < previous
            || event.time < span.start
            || event.time > span.end
        {
            return Err(PowerError::InvalidHistory(
                "events are outside the span or unordered".into(),
            ));
        }
        if history_jumps(history, event.time)? && event.sides.is_none() {
            return Err(PowerError::MissingOneSidedValues(event.id.as_str().into()));
        }
        if event.time > *splits.last().expect("span start exists") {
            splits.push(event.time);
        }
        previous = event.time;
    }
    if events.iter().any(|e| e.terminal)
        && events
            .iter()
            .filter(|e| e.terminal)
            .any(|e| e.time != span.end)
    {
        return Err(PowerError::TerminalEndpointMismatch);
    }
    if splits.last().copied() != Some(span.end) {
        splits.push(span.end);
    }
    let mut segments = Vec::new();
    for pair in splits.windows(2) {
        segments.push(integrate_segment(port, history, pair[0], pair[1], &plan)?);
    }
    let signed_work = segments.iter().map(|s| s.signed_work).sum();
    let uncertainty = segments.iter().map(|s| s.uncertainty).sum();
    Ok(PortWork {
        port: port.clone(),
        source,
        span,
        plan,
        segments,
        signed_work,
        uncertainty,
    })
}

fn integrate_segment(
    port: &ConjugatePort,
    history: &PortHistory,
    start: f64,
    end: f64,
    plan: &ReductionPlan,
) -> Result<SegmentEvidence, PowerError> {
    let points: Vec<PortSample> = match (history, plan) {
        (PortHistory::Callable(h), ReductionPlan::Trapezoid { intervals }) if *intervals > 0 => (0
            ..=*intervals)
            .map(|i| {
                let t = start + (end - start) * (i as f64) / (*intervals as f64);
                let (effort, flow) = (h.sample)(t);
                PortSample {
                    time: t,
                    effort,
                    flow,
                }
            })
            .collect(),
        (PortHistory::Sampled(h), ReductionPlan::SampledTrapezoid) => h
            .samples
            .iter()
            .copied()
            .filter(|s| s.time >= start && s.time <= end)
            .collect(),
        _ => {
            return Err(PowerError::InvalidHistory(
                "reduction plan does not match history".into(),
            ));
        }
    };
    if points.len() < 2
        || points.first().map(|p| p.time) != Some(start)
        || points.last().map(|p| p.time) != Some(end)
        || points.windows(2).any(|p| p[0].time >= p[1].time)
    {
        return Err(PowerError::InvalidHistory(
            "every segment needs ordered endpoint samples".into(),
        ));
    }
    let powers: Vec<f64> = points
        .iter()
        .map(|p| port.signed_power(p.effort, p.flow).0)
        .collect();
    if powers.iter().any(|v| !v.is_finite()) {
        return Err(PowerError::InvalidHistory("non-finite port value".into()));
    }
    let signed_work = points
        .windows(2)
        .zip(powers.windows(2))
        .map(|(t, p)| (t[1].time - t[0].time) * (p[0] + p[1]) * 0.5)
        .sum();
    let uncertainty = points
        .windows(3)
        .zip(powers.windows(3))
        .map(|(t, p)| {
            ((p[2] - p[1]) / (t[2].time - t[1].time) - (p[1] - p[0]) / (t[1].time - t[0].time))
                .abs()
                * (t[2].time - t[0].time).powi(2)
                / 12.0
        })
        .sum();
    Ok(SegmentEvidence {
        start,
        end,
        method: "composite-trapezoid".into(),
        evaluations: points.len(),
        signed_work,
        uncertainty,
    })
}

fn history_jumps(history: &PortHistory, time: f64) -> Result<bool, PowerError> {
    match history {
        PortHistory::Callable(_) => Ok(false),
        PortHistory::Sampled(h) => {
            let at: Vec<_> = h.samples.iter().filter(|s| s.time == time).collect();
            Ok(at.len() > 1
                && at
                    .windows(2)
                    .any(|v| v[0].effort != v[1].effort || v[0].flow != v[1].flow))
        }
    }
}

fn exp(n: i32) -> Exponent {
    Exponent::new(n, 1).expect("small integral exponent")
}
fn dim(t: i32, l: i32, m: i32, i: i32) -> Dimension {
    Dimension::from_exponents([exp(t), exp(l), exp(m), exp(i), exp(0), exp(0), exp(0)])
}
fn power_dimension() -> Dimension {
    dim(-3, 2, 1, 0)
}
fn voltage_dimension() -> Dimension {
    dim(-3, 2, 1, -1)
}
fn current_dimension() -> Dimension {
    Dimension::base(BaseDimension::Current)
}
fn force_dimension() -> Dimension {
    dim(-2, 1, 1, 0)
}
fn velocity_dimension() -> Dimension {
    dim(-1, 1, 0, 0)
}
fn torque_dimension() -> Dimension {
    dim(-2, 2, 1, 0)
}
fn angular_velocity_dimension() -> Dimension {
    dim(-1, 0, 0, 0)
}
fn pressure_dimension() -> Dimension {
    dim(-2, -1, 1, 0)
}
fn volume_flow_dimension() -> Dimension {
    dim(-1, 3, 0, 0)
}
fn builtin(id: &str, e: &str, f: &str, ed: Dimension, fd: Dimension) -> PowerPair {
    PowerPair::new(
        ContentId::new(format!("physics/power-pair/{id}")).unwrap(),
        e,
        f,
        ed,
        fd,
    )
    .unwrap()
}

/// Shapes installed by the loadable power runtime surface.
pub const SHAPES: &[&str] = &[
    "physics/PowerPair",
    "physics/ConjugatePort",
    "physics/PortHistory",
    "physics/PortWork",
    "physics/ImpulseTransfer",
];
/// Loadable runtime export names.
pub const RUNTIME_EXPORTS: &[&str] = &[
    "physics/power",
    "physics/integrate-port-work",
    "physics/audit-port-work",
];
