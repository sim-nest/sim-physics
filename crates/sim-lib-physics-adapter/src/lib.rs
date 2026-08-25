#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Domain-neutral records and fail-closed conformance for physics model adapters.
//!
//! Domain solvers lower records into this crate; this crate never depends on a
//! field, mesh, phasor, wave, solver, host, or device implementation.

use std::collections::BTreeSet;

/// Stable identity of an adapted domain model or state.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableIdentity(String);
impl StableIdentity {
    /// Admits a non-empty portable identity.
    pub fn new(value: impl Into<String>) -> Result<Self, AdapterRefusal> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/' | b'.' | b':'))
        {
            return Err(AdapterRefusal::InvalidIdentity(value));
        }
        Ok(Self(value))
    }
    /// Returns the stable representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether data describes a model prediction or an observation of reality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataOrigin {
    /// Computed from a model.
    Modeled,
    /// Obtained through an external provider and supplied as a record.
    Observed,
}

/// Exact SI base-dimension exponents in mass, length, time, current, temperature, amount, luminous intensity order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dimension(pub [i8; 7]);

/// A semantic physical observation. `kind` prevents equal dimensions from implying equal meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Stable observation identity.
    pub id: StableIdentity,
    /// Open semantic kind, such as `si:energy` or `wave:amplitude-squared`.
    pub kind: String,
    /// SI base dimension.
    pub dimension: Dimension,
    /// Finite scalar expressed in the named unit.
    pub value: f64,
    /// Unit symbol.
    pub unit: String,
    /// Data origin.
    pub origin: DataOrigin,
}

/// Explicit conjugate effort/flow declaration for a lumped boundary port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortDeclaration {
    /// Stable port identity.
    pub id: StableIdentity,
    /// Effort semantic kind.
    pub effort_kind: String,
    /// Flow semantic kind.
    pub flow_kind: String,
}

/// Complete domain lowering accepted by physics audits.
#[derive(Clone, Debug, PartialEq)]
pub struct AdaptedModel {
    /// Stable domain model identity.
    pub model_id: StableIdentity,
    /// Stable state/solution identity.
    pub state_id: StableIdentity,
    /// Explicit boundary identity.
    pub boundary_id: StableIdentity,
    /// Whether the adapter asserts that all boundary exchanges are enumerated.
    pub boundary_complete: bool,
    /// Declared lumped conjugate ports; wave samples are not ports.
    pub ports: Vec<PortDeclaration>,
    /// Stable store identities.
    pub stores: Vec<StableIdentity>,
    /// Stable event identities.
    pub events: Vec<StableIdentity>,
    /// Semantic observations.
    pub observations: Vec<Observation>,
    /// State identities influenced by this result.
    pub influences: Vec<StableIdentity>,
    /// Evidence about model/formulation assumptions only.
    pub model_evidence: Vec<String>,
    /// Evidence emitted by solver, sampler, or compute provider only.
    pub solver_evidence: Vec<String>,
}

/// Fail-closed adapter refusal.
#[derive(Clone, Debug, PartialEq)]
pub enum AdapterRefusal {
    /// Invalid stable identity.
    InvalidIdentity(String),
    /// Boundary completeness was not established.
    IncompleteBoundary,
    /// An audit required a conjugate port but none was declared.
    MissingPort,
    /// A scalar was non-finite or semantic metadata was empty.
    InvalidObservation(String),
    /// A stable identity was repeated.
    DuplicateIdentity(String),
    /// Model and solver evidence were not independently supplied.
    EvidenceNotSeparated,
}

impl AdaptedModel {
    /// Validates completeness, uniqueness, quantity metadata, and evidence separation.
    pub fn validate(&self) -> Result<(), AdapterRefusal> {
        if !self.boundary_complete {
            return Err(AdapterRefusal::IncompleteBoundary);
        }
        if self.model_evidence.is_empty() || self.solver_evidence.is_empty() {
            return Err(AdapterRefusal::EvidenceNotSeparated);
        }
        let mut ids = BTreeSet::new();
        for id in self
            .ports
            .iter()
            .map(|v| &v.id)
            .chain(self.stores.iter())
            .chain(self.events.iter())
        {
            if !ids.insert(id) {
                return Err(AdapterRefusal::DuplicateIdentity(id.as_str().into()));
            }
        }
        for value in &self.observations {
            if !value.value.is_finite() || value.kind.is_empty() || value.unit.is_empty() {
                return Err(AdapterRefusal::InvalidObservation(value.id.as_str().into()));
            }
        }
        Ok(())
    }
    /// Validates the model and requires an explicit conjugate port.
    pub fn validate_lumped_audit(&self) -> Result<(), AdapterRefusal> {
        self.validate()?;
        if self.ports.is_empty() {
            return Err(AdapterRefusal::MissingPort);
        }
        Ok(())
    }
}

/// SI energy dimension: mass length² time⁻².
pub const ENERGY: Dimension = Dimension([1, 2, -2, 0, 0, 0, 0]);
/// SI power dimension: mass length² time⁻³.
pub const POWER: Dimension = Dimension([1, 2, -3, 0, 0, 0, 0]);
/// Dimensionless scalar.
pub const DIMENSIONLESS: Dimension = Dimension([0; 7]);
