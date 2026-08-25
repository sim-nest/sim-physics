//! Orthogonal refinement and certified model verdicts.

use sim_lib_numbers_interval::{
    CertifiedInterval, EstimateInterval, ThresholdVerdict, classify_estimate, classify_threshold,
};
use std::collections::BTreeSet;
use std::fmt::Write;

/// Typed links to the composed audit, precision, and numerical-method owners.
///
/// The proof layer interprets these records but does not reimplement them.
pub struct ComposedEvidence<'a> {
    pub audit: &'a sim_lib_physics_audit::AuditRecord,
    #[cfg(feature = "extended-precision")]
    pub extended_scalar: sim_lib_numbers_extended::DoubleDouble,
    pub method: &'a sim_lib_numbers_method::MethodEvidence,
}

/// Every independently reviewable refinement axis.
#[derive(Clone, Debug, PartialEq)]
pub struct RefinementPlan {
    pub time_step_or_tolerance: String,
    pub output_quadrature_mesh: String,
    pub scalar_precision: String,
    pub numerical_method: String,
    pub parameter_enclosure: String,
    pub model_variant: String,
}

/// A located discontinuity and its exact coordinate.
#[derive(Clone, Debug, PartialEq)]
pub struct LocatedEvent {
    pub id: String,
    pub at: f64,
}

/// Side retained at a mesh coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Continuous,
    Left,
    Right,
}

/// One event-exact output request.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshPoint {
    pub at: f64,
    pub side: Side,
    pub event_id: Option<String>,
}

/// Builds a sorted mesh and duplicates each event coordinate for both limits.
pub fn event_exact_mesh(
    span: (f64, f64),
    regular: &[f64],
    events: &[LocatedEvent],
) -> Result<Vec<MeshPoint>, String> {
    if !span.0.is_finite() || !span.1.is_finite() || span.0 > span.1 {
        return Err("invalid declared span".into());
    }
    let mut out = Vec::new();
    for &at in regular {
        if !at.is_finite() || at < span.0 || at > span.1 {
            return Err(format!("regular output {at} lies outside declared span"));
        }
        out.push(MeshPoint {
            at,
            side: Side::Continuous,
            event_id: None,
        });
    }
    let mut ids = BTreeSet::new();
    for event in events {
        if !ids.insert(event.id.clone()) {
            return Err(format!("duplicate event id {}", event.id));
        }
        if !event.at.is_finite() || event.at < span.0 || event.at > span.1 {
            return Err(format!("event {} lies outside declared span", event.id));
        }
        out.push(MeshPoint {
            at: event.at,
            side: Side::Left,
            event_id: Some(event.id.clone()),
        });
        out.push(MeshPoint {
            at: event.at,
            side: Side::Right,
            event_id: Some(event.id.clone()),
        });
    }
    out.sort_by(|a, b| {
        a.at.total_cmp(&b.at)
            .then(side_rank(a.side).cmp(&side_rank(b.side)))
    });
    Ok(out)
}

fn side_rank(side: Side) -> u8 {
    match side {
        Side::Left => 0,
        Side::Continuous => 1,
        Side::Right => 2,
    }
}

/// Signed comparison between two evidence-bearing results.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    pub signed_delta: f64,
    pub absolute_delta: f64,
    pub relative_delta: Option<f64>,
    pub coarse_evidence: String,
    pub fine_evidence: String,
}

/// Compares two levels without losing direction or provenance.
pub fn compare_levels(
    coarse: f64,
    fine: f64,
    coarse_evidence: &str,
    fine_evidence: &str,
) -> Comparison {
    let signed_delta = fine - coarse;
    Comparison {
        signed_delta,
        absolute_delta: signed_delta.abs(),
        relative_delta: (fine != 0.0).then_some(signed_delta.abs() / fine.abs()),
        coarse_evidence: coarse_evidence.into(),
        fine_evidence: fine_evidence.into(),
    }
}

/// Independent acceptance lanes; `None` means not applicable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GateValues {
    pub solver: Option<f64>,
    pub quadrature: Option<f64>,
    pub model: Option<f64>,
    pub balance: Option<f64>,
    pub absolute_work: Option<f64>,
    pub relative_work: Option<f64>,
}

/// Full lane-by-lane gate result.
#[derive(Clone, Debug, PartialEq)]
pub struct GateReport {
    pub solver: Option<bool>,
    pub quadrature: Option<bool>,
    pub model: Option<bool>,
    pub balance: Option<bool>,
    pub absolute_work: Option<bool>,
    pub relative_work: Option<bool>,
    pub all_applicable_pass: bool,
}

/// Applies thresholds independently, without weighting or aggregation.
pub fn apply_thresholds(values: &GateValues, limits: &GateValues) -> Result<GateReport, String> {
    fn lane(name: &str, value: Option<f64>, limit: Option<f64>) -> Result<Option<bool>, String> {
        match (value, limit) {
            (None, None) => Ok(None),
            (Some(v), Some(l)) if v.is_finite() && l.is_finite() && l >= 0.0 => {
                Ok(Some(v.abs() <= l))
            }
            (Some(_), None) | (None, Some(_)) => {
                Err(format!("{name} value and threshold applicability differ"))
            }
            _ => Err(format!("invalid {name} gate")),
        }
    }
    let solver = lane("solver", values.solver, limits.solver)?;
    let quadrature = lane("quadrature", values.quadrature, limits.quadrature)?;
    let model = lane("model", values.model, limits.model)?;
    let balance = lane("balance", values.balance, limits.balance)?;
    let absolute_work = lane("absolute work", values.absolute_work, limits.absolute_work)?;
    let relative_work = lane("relative work", values.relative_work, limits.relative_work)?;
    let all_applicable_pass = [
        solver,
        quadrature,
        model,
        balance,
        absolute_work,
        relative_work,
    ]
    .into_iter()
    .flatten()
    .all(|passed| passed);
    Ok(GateReport {
        solver,
        quadrature,
        model,
        balance,
        absolute_work,
        relative_work,
        all_applicable_pass,
    })
}

/// Diagnosis from method and precision continuation over unchanged study input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContinuationOutcome {
    SolverLimited,
    PrecisionLimited,
    ModelSensitive,
    Stable,
    Unresolved,
}

/// Evidence supplied by continuation runs.
#[derive(Clone, Debug, PartialEq)]
pub struct ContinuationEvidence {
    pub unchanged_study_input: bool,
    pub time_refinement_converged: Option<bool>,
    pub precision_change_resolved: Option<bool>,
    pub methods_agree: Option<bool>,
    pub models_agree: Option<bool>,
}

/// Classifies limitation only when the relevant independent evidence exists.
pub fn classify_continuation(e: &ContinuationEvidence) -> ContinuationOutcome {
    if !e.unchanged_study_input {
        return ContinuationOutcome::Unresolved;
    }
    match (
        e.time_refinement_converged,
        e.precision_change_resolved,
        e.methods_agree,
        e.models_agree,
    ) {
        (_, _, _, Some(false)) => ContinuationOutcome::ModelSensitive,
        (Some(false), _, Some(false), _) => ContinuationOutcome::SolverLimited,
        (_, Some(true), _, _) => ContinuationOutcome::PrecisionLimited,
        (Some(true), Some(false), Some(true), Some(true)) => ContinuationOutcome::Stable,
        _ => ContinuationOutcome::Unresolved,
    }
}

/// Threshold input whose certification status remains explicit.
pub enum VerdictInput<'a> {
    Certified(&'a CertifiedInterval),
    Estimate(EstimateInterval),
}

/// Produces a definite result only for two certified enclosures.
pub fn certified_verdict(value: VerdictInput<'_>, threshold: VerdictInput<'_>) -> ThresholdVerdict {
    match (value, threshold) {
        (VerdictInput::Certified(v), VerdictInput::Certified(t)) => classify_threshold(v, t),
        (VerdictInput::Certified(v), VerdictInput::Estimate(t)) => classify_estimate(
            EstimateInterval::new(v.lower(), v.upper()).expect("a certificate is ordered"),
            t,
        ),
        (VerdictInput::Estimate(v), VerdictInput::Certified(t)) => classify_estimate(
            v,
            EstimateInterval::new(t.lower(), t.upper()).expect("a certificate is ordered"),
        ),
        (VerdictInput::Estimate(v), VerdictInput::Estimate(t)) => classify_estimate(v, t),
    }
}

/// Physical validation is deliberately separate from mathematical enclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicalValidation {
    NotAssessed,
    Compared { measurement_evidence: String },
    Validated { measurement_evidence: String },
}

/// Immutable, reproducibly identified mathematical proof record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofRecord {
    pub proof_id: String,
    pub mathematical_model: String,
    pub parameter_enclosure_id: String,
    pub numerical_evidence_ids: Vec<String>,
    pub verdict: String,
    pub physical_validation: PhysicalValidation,
}

impl ProofRecord {
    /// Constructs a proof record whose mathematical identity excludes measurement status.
    pub fn new(
        model: &str,
        enclosure: &str,
        mut evidence: Vec<String>,
        verdict: &ThresholdVerdict,
        physical_validation: PhysicalValidation,
    ) -> Result<Self, String> {
        if model.trim().is_empty()
            || enclosure.trim().is_empty()
            || evidence.iter().any(|x| x.trim().is_empty())
        {
            return Err("proof identities must be non-empty".into());
        }
        evidence.sort();
        evidence.dedup();
        let verdict = match verdict {
            ThresholdVerdict::Below => "below",
            ThresholdVerdict::Above => "above",
            ThresholdVerdict::Unresolved(_) => "unresolved",
        }
        .to_owned();
        let mut canonical = format!("model={model}\nenclosure={enclosure}\nverdict={verdict}\n");
        for id in &evidence {
            let _ = writeln!(canonical, "evidence={id}");
        }
        let proof_id = format!(
            "physics-proof-v1-{:016x}",
            stable_hash(canonical.as_bytes())
        );
        Ok(Self {
            proof_id,
            mathematical_model: model.into(),
            parameter_enclosure_id: enclosure.into(),
            numerical_evidence_ids: evidence,
            verdict,
            physical_validation,
        })
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325_u64;
    for &byte in bytes {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Loadable runtime names exposed by this library.
pub const RUNTIME_EXPORTS: &[&str] = &[
    "physics/refinement-plan",
    "physics/event-exact-mesh",
    "physics/certified-verdict",
    "physics/proof-record",
];
