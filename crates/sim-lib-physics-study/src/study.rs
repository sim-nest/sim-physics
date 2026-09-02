use sha2::{Digest, Sha256};
use sim_kernel::{
    CapabilityName, Consistency, ContentId, Cx, EvalFabric, EvalMode, EvalRequest, Expr, ShapeRef,
    Symbol, Value,
};
use sim_lib_numbers_stats::{
    CoverageEvidence, LatinHypercubePlan, SampleDesign, SobolPlan, SweepPlan,
};
pub use sim_lib_numbers_stats::{Scramble, UntestedRegion};
use sim_lib_physics_influence::SelectionInput;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

/// Stable identity used for reviewable plan components and execution records.
pub type Identity = ContentId;

/// Which disjoint statistical lane owns a point.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Partition {
    Fit,
    Selection,
    Test,
}

/// Unit-cube sampling policy. Grid counts are per axis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SamplerPolicy {
    Grid {
        counts: Vec<usize>,
    },
    Latin {
        points: usize,
    },
    Sobol {
        points: usize,
        skip: u64,
        scramble: Scramble,
    },
}

/// Semantic spacing applied after canonical unit-cube generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Spacing {
    Linear,
    Logarithmic,
}

/// One exact injection, expressed in physical coordinates.
#[derive(Clone)]
pub struct BoundaryInjection {
    pub label: String,
    pub values: Vec<f64>,
}

/// A semantic quantity axis. `shape` is the canonical runtime Shape.
#[derive(Clone)]
pub struct ParameterAxis {
    pub name: String,
    pub quantity_shape: ShapeRef,
    pub inclusive_bounds: (f64, f64),
    pub spacing: Spacing,
}

/// Deterministic partition assignment. Every generated ordinal is assigned once.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionPlan {
    pub fit: BTreeSet<usize>,
    pub selection: BTreeSet<usize>,
    pub test: BTreeSet<usize>,
}

/// Placement request remains data; EvalFabric resolves the provider late.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementRequest {
    pub target: Symbol,
    pub consistency: Consistency,
    pub required_capabilities: Vec<CapabilityName>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StudyLimits {
    pub max_points: usize,
    pub max_failures: usize,
    pub deadline: Option<Duration>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditPolicy {
    pub energy_after_selection_only: bool,
    pub record_residuals: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InfluencePolicy {
    pub require_clean_selection: bool,
}

/// All inputs to a study, immutable after validation.
#[derive(Clone)]
pub struct StudyPlan {
    pub id: Identity,
    pub model_id: Identity,
    pub boundary_event_graph_id: Identity,
    pub initial_state_id: Identity,
    pub method_plan_ids: Vec<Identity>,
    pub refinement_plan_ids: Vec<Identity>,
    pub audit_policy: AuditPolicy,
    pub influence_policy: InfluencePolicy,
    pub outputs: Vec<String>,
    pub limits: StudyLimits,
    pub seed: u64,
    pub placement: PlacementRequest,
    pub axes: Vec<ParameterAxis>,
    pub sampler: SamplerPolicy,
    pub boundary_injections: Vec<BoundaryInjection>,
    pub partitions: PartitionPlan,
    pub untested_regions: Vec<UntestedRegion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanError {
    Empty(&'static str),
    InvalidAxis(String),
    InvalidBoundary(String),
    PartitionOverlap(usize),
    MissingPartition(usize),
    WorkLimit { requested: usize, limit: usize },
    Sampler(String),
}

impl StudyPlan {
    /// Validates the full immutable binding before sampling or execution.
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.axes.is_empty() {
            return Err(PlanError::Empty("axes"));
        }
        if self.outputs.is_empty() {
            return Err(PlanError::Empty("outputs"));
        }
        if self.method_plan_ids.is_empty() {
            return Err(PlanError::Empty("method plans"));
        }
        if self.refinement_plan_ids.is_empty() {
            return Err(PlanError::Empty("refinement plans"));
        }
        if self.untested_regions.is_empty() {
            return Err(PlanError::Empty("known untested regions"));
        }
        if !self.audit_policy.energy_after_selection_only {
            return Err(PlanError::Empty("energy audit must be post-selection"));
        }
        for axis in &self.axes {
            let (lo, hi) = axis.inclusive_bounds;
            if axis.name.is_empty()
                || !lo.is_finite()
                || !hi.is_finite()
                || lo > hi
                || (axis.spacing == Spacing::Logarithmic && lo <= 0.0)
            {
                return Err(PlanError::InvalidAxis(axis.name.clone()));
            }
        }
        for boundary in &self.boundary_injections {
            if boundary.label.is_empty()
                || boundary.values.len() != self.axes.len()
                || boundary.values.iter().zip(&self.axes).any(|(v, a)| {
                    !v.is_finite() || *v < a.inclusive_bounds.0 || *v > a.inclusive_bounds.1
                })
            {
                return Err(PlanError::InvalidBoundary(boundary.label.clone()));
            }
        }
        for index in self
            .partitions
            .fit
            .iter()
            .chain(&self.partitions.selection)
            .chain(&self.partitions.test)
        {
            let lanes = usize::from(self.partitions.fit.contains(index))
                + usize::from(self.partitions.selection.contains(index))
                + usize::from(self.partitions.test.contains(index));
            if lanes > 1 {
                return Err(PlanError::PartitionOverlap(*index));
            }
        }
        Ok(())
    }

    /// Generates points through the canonical stats designs and sweep evidence.
    pub fn design(&self) -> Result<StudyDesign, PlanError> {
        self.validate()?;
        let dimensions = self.axes.len();
        let base = match &self.sampler {
            SamplerPolicy::Grid { counts } => {
                grid_design(dimensions, counts, &self.untested_regions)?
            }
            SamplerPolicy::Latin { points } => LatinHypercubePlan {
                dimensions,
                points: *points,
                seed: self.seed,
                max_work: self.limits.max_points.saturating_mul(dimensions) as u64,
                untested_regions: self.untested_regions.clone(),
            }
            .generate()
            .map_err(|e| PlanError::Sampler(format!("{e:?}")))?,
            SamplerPolicy::Sobol {
                points,
                skip,
                scramble,
            } => SobolPlan {
                dimensions,
                points: *points,
                skip: *skip,
                scramble: *scramble,
                seed: self.seed,
                max_work: self.limits.max_points.saturating_mul(dimensions) as u64,
                untested_regions: self.untested_regions.clone(),
            }
            .generate()
            .map_err(|e| PlanError::Sampler(format!("{e:?}")))?,
        };
        // Canonical stats owner computes exact duplicate evidence for all designs.
        let mut sampled = SweepPlan {
            inject_lower_boundary: false,
            inject_upper_boundary: false,
            untested_regions: vec![],
        }
        .apply(base);
        let mut physical = sampled
            .points
            .iter()
            .map(|p| scale_point(p, &self.axes))
            .collect::<Vec<_>>();
        let mut injected = Vec::new();
        for boundary in &self.boundary_injections {
            if let Some(existing) = physical
                .iter()
                .position(|p| exact_point_eq(p, &boundary.values))
            {
                injected.push((boundary.label.clone(), existing, true));
            } else {
                let index = physical.len();
                physical.push(boundary.values.clone());
                sampled
                    .points
                    .push(unscale_point(&boundary.values, &self.axes));
                injected.push((boundary.label.clone(), index, false));
            }
        }
        if physical.len() > self.limits.max_points {
            return Err(PlanError::WorkLimit {
                requested: physical.len(),
                limit: self.limits.max_points,
            });
        }
        sampled.coverage.boundary_injections = injected.iter().map(|(_, i, _)| *i).collect();
        sampled.coverage.duplicates = exact_duplicates(&physical);
        let mut points = Vec::with_capacity(physical.len());
        for (ordinal, coordinates) in physical.into_iter().enumerate() {
            let partition = partition_for(&self.partitions, ordinal)
                .ok_or(PlanError::MissingPartition(ordinal))?;
            points.push(StudyPoint {
                id: point_id(&self.id, ordinal, &coordinates),
                ordinal,
                coordinates,
                partition,
            });
        }
        Ok(StudyDesign {
            plan_id: self.id.clone(),
            points,
            coverage: sampled.coverage,
            boundary_evidence: injected,
        })
    }
}

fn partition_for(plan: &PartitionPlan, i: usize) -> Option<Partition> {
    if plan.fit.contains(&i) {
        Some(Partition::Fit)
    } else if plan.selection.contains(&i) {
        Some(Partition::Selection)
    } else if plan.test.contains(&i) {
        Some(Partition::Test)
    } else {
        None
    }
}

fn grid_design(
    dimensions: usize,
    counts: &[usize],
    untested: &[UntestedRegion],
) -> Result<SampleDesign, PlanError> {
    if counts.len() != dimensions || counts.contains(&0) {
        return Err(PlanError::Sampler(
            "grid counts must match nonzero axes".into(),
        ));
    }
    let count = counts
        .iter()
        .try_fold(1usize, |n, v| n.checked_mul(*v))
        .ok_or(PlanError::Sampler("grid size overflow".into()))?;
    let mut points = Vec::with_capacity(count);
    for ordinal in 0..count {
        let mut remainder = ordinal;
        let mut point = Vec::with_capacity(dimensions);
        for cells in counts {
            let cell = remainder % *cells;
            remainder /= *cells;
            point.push(if *cells == 1 {
                0.5
            } else {
                cell as f64 / (*cells - 1) as f64
            });
        }
        points.push(point);
    }
    Ok(SampleDesign {
        points,
        coverage: CoverageEvidence {
            sequence_identity: format!("cartesian-grid/inclusive-v1;counts={counts:?}"),
            boundary_injections: vec![],
            duplicates: vec![],
            stratum_occupancy: counts.iter().map(|n| vec![1; *n]).collect(),
            sampler: None,
            untested_regions: untested.to_vec(),
            work: count as u64,
        },
    })
}

fn scale_point(unit: &[f64], axes: &[ParameterAxis]) -> Vec<f64> {
    unit.iter()
        .zip(axes)
        .map(|(u, a)| {
            let (lo, hi) = a.inclusive_bounds;
            match a.spacing {
                Spacing::Linear => lo + u * (hi - lo),
                Spacing::Logarithmic => (lo.ln() + u * (hi.ln() - lo.ln())).exp(),
            }
        })
        .collect()
}
fn unscale_point(values: &[f64], axes: &[ParameterAxis]) -> Vec<f64> {
    values
        .iter()
        .zip(axes)
        .map(|(v, a)| {
            let (lo, hi) = a.inclusive_bounds;
            if lo == hi {
                0.0
            } else {
                match a.spacing {
                    Spacing::Linear => (v - lo) / (hi - lo),
                    Spacing::Logarithmic => (v.ln() - lo.ln()) / (hi.ln() - lo.ln()),
                }
            }
        })
        .collect()
}
fn exact_point_eq(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}
fn exact_duplicates(points: &[Vec<f64>]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    for later in 0..points.len() {
        if let Some(earlier) = (0..later).find(|i| exact_point_eq(&points[*i], &points[later])) {
            out.push((later, earlier));
        }
    }
    out
}

fn identity(parts: impl IntoIterator<Item = impl AsRef<[u8]>>) -> Identity {
    let mut h = Sha256::new();
    for part in parts {
        let p = part.as_ref();
        h.update((p.len() as u64).to_be_bytes());
        h.update(p);
    }
    ContentId::from_bytes(Symbol::qualified("core", "sha256"), h.finalize().into())
}
fn point_id(plan: &Identity, ordinal: usize, values: &[f64]) -> Identity {
    let mut parts = Vec::new();
    parts.push(plan.bytes.to_vec());
    parts.push((ordinal as u64).to_be_bytes().to_vec());
    for value in values {
        parts.push(value.to_bits().to_be_bytes().to_vec());
    }
    identity(parts)
}

/// Generated point set plus exact sampler and coverage evidence.
pub struct StudyDesign {
    pub plan_id: Identity,
    pub points: Vec<StudyPoint>,
    pub coverage: CoverageEvidence,
    pub boundary_evidence: Vec<(String, usize, bool)>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StudyPoint {
    pub id: Identity,
    pub ordinal: usize,
    pub coordinates: Vec<f64>,
    pub partition: Partition,
}

/// Caller-owned expression builder; study owns orchestration, not model encoding.
pub trait PointExpression: Send + Sync {
    fn expression(&self, plan: &StudyPlan, point: &StudyPoint) -> sim_kernel::Result<Expr>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IncompleteOutcome {
    Refused(String),
    Interrupted(String),
    Unknown(String),
}

/// One placement-transparent execution outcome.
pub enum PointOutcome {
    Complete(Value),
    Incomplete(IncompleteOutcome),
}

pub struct PointResult {
    pub id: Identity,
    pub plan_id: Identity,
    pub point_id: Identity,
    pub reference_execution_id: Identity,
    pub provider_id: Identity,
    pub partition: Partition,
    pub outcome: PointOutcome,
}

/// Executes points sequentially only to submit independent ordinary EvalFabric requests.
/// Concurrency, transport, retry, and site choice remain exclusively fabric concerns.
pub fn realize_sweep(
    cx: &mut Cx,
    fabric: &dyn EvalFabric,
    plan: &StudyPlan,
    design: &StudyDesign,
    builder: &dyn PointExpression,
    provider_id: Identity,
) -> Vec<PointResult> {
    let mut results = Vec::with_capacity(design.points.len());
    for point in &design.points {
        let execution_id = identity([
            plan.id.bytes.as_slice(),
            point.id.bytes.as_slice(),
            provider_id.bytes.as_slice(),
        ]);
        let outcome = match builder.expression(plan, point) {
            Err(error) => PointOutcome::Incomplete(IncompleteOutcome::Refused(error.to_string())),
            Ok(expr) => match fabric.realize(
                cx,
                EvalRequest {
                    expr,
                    result_shape: None,
                    required_capabilities: plan.placement.required_capabilities.clone(),
                    deadline: plan.limits.deadline,
                    consistency: plan.placement.consistency,
                    mode: EvalMode::Eval,
                    answer_limit: None,
                    stream_buffer: None,
                    stream: false,
                    trace: true,
                },
            ) {
                Ok(reply) => PointOutcome::Complete(reply.value),
                Err(error) => PointOutcome::Incomplete(classify_failure(error.to_string())),
            },
        };
        let result_id = identity([execution_id.bytes.as_slice(), outcome_tag(&outcome)]);
        results.push(PointResult {
            id: result_id,
            plan_id: plan.id.clone(),
            point_id: point.id.clone(),
            reference_execution_id: execution_id,
            provider_id: provider_id.clone(),
            partition: point.partition,
            outcome,
        });
    }
    results
}
fn classify_failure(message: String) -> IncompleteOutcome {
    let lower = message.to_ascii_lowercase();
    if lower.contains("interrupt") || lower.contains("cancel") {
        IncompleteOutcome::Interrupted(message)
    } else if lower.contains("refus") || lower.contains("capab") || lower.contains("limit") {
        IncompleteOutcome::Refused(message)
    } else {
        IncompleteOutcome::Unknown(message)
    }
}
fn outcome_tag(outcome: &PointOutcome) -> &'static [u8] {
    match outcome {
        PointOutcome::Complete(_) => b"complete",
        PointOutcome::Incomplete(IncompleteOutcome::Refused(_)) => b"refused",
        PointOutcome::Incomplete(IncompleteOutcome::Interrupted(_)) => b"interrupted",
        PointOutcome::Incomplete(IncompleteOutcome::Unknown(_)) => b"unknown",
    }
}

/// Clean selection proof and candidates. There is deliberately no energy input.
pub fn select<'a>(
    proof: &SelectionInput,
    candidates: impl IntoIterator<Item = &'a PointResult>,
    score: impl Fn(&PointResult) -> Option<f64>,
) -> Option<&'a PointResult> {
    let _proof_identity = proof.proof_identity();
    candidates
        .into_iter()
        .filter(|r| r.partition == Partition::Selection)
        .filter_map(|r| score(r).filter(|v| v.is_finite()).map(|v| (r, v)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(r, _)| r)
}

/// Post-selection observations. These can annotate but cannot choose a result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Observations {
    pub energy_store_change: Option<f64>,
    pub unexplained_residual: Option<f64>,
    pub sign_reversal: bool,
    pub zero_crossing: bool,
    pub event_transition: bool,
    pub topology_transition: bool,
    pub initial_state_sensitivity: Option<f64>,
    pub boundary_value: bool,
    pub fold: bool,
    pub method_sensitivity: Option<f64>,
    pub tolerance_sensitivity: Option<f64>,
    pub precision_sensitivity: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct ObservationInput {
    pub energy_before: Option<f64>,
    pub energy_after: Option<f64>,
    pub residual: Option<f64>,
    pub previous: Option<f64>,
    pub current: Option<f64>,
    pub event_changed: bool,
    pub topology_changed: bool,
    pub initial_delta: Option<f64>,
    pub boundary_value: bool,
    pub slope_before: Option<f64>,
    pub slope_after: Option<f64>,
    pub method_delta: Option<f64>,
    pub tolerance_delta: Option<f64>,
    pub precision_delta: Option<f64>,
}
pub fn detect(input: &ObservationInput) -> Observations {
    let sign_reversal = matches!((input.previous,input.current),(Some(a),Some(b)) if a.signum()!=b.signum() && a!=0.0 && b!=0.0);
    let zero_crossing = matches!((input.previous,input.current),(Some(a),Some(b)) if a==0.0 || b==0.0 || a.signum()!=b.signum());
    let fold = matches!((input.slope_before,input.slope_after),(Some(a),Some(b)) if a.signum()!=b.signum());
    Observations {
        energy_store_change: input
            .energy_before
            .zip(input.energy_after)
            .map(|(a, b)| b - a),
        unexplained_residual: input.residual,
        sign_reversal,
        zero_crossing,
        event_transition: input.event_changed,
        topology_transition: input.topology_changed,
        initial_state_sensitivity: input.initial_delta.map(f64::abs),
        boundary_value: input.boundary_value,
        fold,
        method_sensitivity: input.method_delta.map(f64::abs),
        tolerance_sensitivity: input.tolerance_delta.map(f64::abs),
        precision_sensitivity: input.precision_delta.map(f64::abs),
    }
}

/// Replay is exact only under the same reference execution identity.
pub fn exact_replay(left: &PointResult, right: &PointResult) -> bool {
    left.reference_execution_id == right.reference_execution_id && left.id == right.id
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderEnvelope {
    pub provider_id: Identity,
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EnvelopeComparison {
    pub equivalent: bool,
    pub absolute_delta: f64,
    pub allowed_delta: f64,
}
pub fn compare_provider_values(
    left: f64,
    right: f64,
    left_provider: &ProviderEnvelope,
    right_provider: &ProviderEnvelope,
) -> EnvelopeComparison {
    let absolute_delta = (right - left).abs();
    let scale = left.abs().max(right.abs());
    let allowed_delta = left_provider
        .absolute_tolerance
        .max(right_provider.absolute_tolerance)
        + scale
            * left_provider
                .relative_tolerance
                .max(right_provider.relative_tolerance);
    EnvelopeComparison {
        equivalent: absolute_delta <= allowed_delta,
        absolute_delta,
        allowed_delta,
    }
}

/// Summary retains all partial outcomes and never manufactures retry safety.
pub struct StudyResults {
    pub plan_id: Identity,
    pub results: Vec<PointResult>,
    pub observations: BTreeMap<Identity, Observations>,
    pub retry_safe: BTreeSet<Identity>,
}
impl StudyResults {
    pub fn new(plan_id: Identity, results: Vec<PointResult>) -> Self {
        Self {
            plan_id,
            results,
            observations: BTreeMap::new(),
            retry_safe: BTreeSet::new(),
        }
    }
    pub fn annotate_selected(&mut self, result: &PointResult, observations: Observations) {
        self.observations.insert(result.id.clone(), observations);
    }
}

/// Runtime-facing loadable library identity and stable callable names.
pub fn study_lib_symbol() -> Symbol {
    Symbol::qualified("physics-study", "lib")
}
pub fn study_surface_symbols() -> [Symbol; 4] {
    [
        Symbol::qualified("physics-study", "design"),
        Symbol::qualified("physics-study", "realize"),
        Symbol::qualified("physics-study", "select"),
        Symbol::qualified("physics-study", "compare-providers"),
    ]
}

/// Shared fabric handle accepted by host adapters without exposing transport.
pub type StudyFabric = Arc<dyn EvalFabric>;
