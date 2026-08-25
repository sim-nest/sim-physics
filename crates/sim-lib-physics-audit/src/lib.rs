#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Independent stored-energy audits with non-interchangeable evidence lanes.

use std::fmt;

use sim_kernel::{
    Claim, ClaimKind, ContentId as KernelContentId, Datum, NumberLiteral, Ref, Symbol,
};
pub use sim_lib_numbers_method::{CanonicalDatum as MethodCanonicalDatum, MethodEvidence};
pub use sim_lib_numbers_quantity::Dimension as PhysicalDimension;
pub use sim_lib_physics_core::BoundaryId;
pub use sim_lib_physics_power::{
    ContentId, ImpulseKind, ImpulseTransfer, PortWork, PositiveDirection, ReductionPlan, WorkSpan,
};

/// A stored-energy value in canonical joules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredEnergy(pub f64);

/// Input to a constitutive stored-energy evaluator.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstitutiveState {
    /// Immutable state content identity.
    pub state_id: ContentId,
    /// Explicit canonical state coordinates consumed by the evaluator.
    pub coordinates: Vec<f64>,
}

/// Identity and output of one endpoint constitutive evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreEvaluation {
    /// Evaluator implementation identity.
    pub evaluator_id: ContentId,
    /// Constitutive model identity.
    pub model_id: ContentId,
    /// Input state content identity.
    pub state_id: ContentId,
    /// Independently evaluated stored energy.
    pub energy: StoredEnergy,
    /// Evidence supporting this evaluation.
    pub evidence: Vec<ContentId>,
}

/// Evaluates stored energy from constitutive state, independently of transfers.
pub trait StoreEvaluator {
    /// Stable evaluator implementation identity.
    fn evaluator_id(&self) -> &ContentId;
    /// Stable constitutive model identity.
    fn model_id(&self) -> &ContentId;
    /// Evaluates one endpoint state without access to prior energy or work.
    fn evaluate(&self, state: &ConstitutiveState) -> Result<StoredEnergy, AuditError>;
}

/// Evaluates and records one endpoint using only the supplied constitutive state.
pub fn evaluate_store(
    evaluator: &dyn StoreEvaluator,
    state: &ConstitutiveState,
    evidence: Vec<ContentId>,
) -> Result<StoreEvaluation, AuditError> {
    if state.coordinates.iter().any(|value| !value.is_finite()) {
        return Err(AuditError::NonFinite("constitutive state"));
    }
    let energy = evaluator.evaluate(state)?;
    if !energy.0.is_finite() {
        return Err(AuditError::NonFinite("stored energy"));
    }
    Ok(StoreEvaluation {
        evaluator_id: evaluator.evaluator_id().clone(),
        model_id: evaluator.model_id().clone(),
        state_id: state.state_id.clone(),
        energy,
        evidence,
    })
}

/// Explicit reduction of retained transfer terms.
#[derive(Clone, Debug, PartialEq)]
pub struct CompensatedReduction {
    /// Reduction algorithm identity.
    pub method_id: ContentId,
    /// Ordered continuous-work addends, positive into the system.
    pub port_terms: Vec<f64>,
    /// Ordered event-transfer addends, positive into the system.
    pub event_terms: Vec<f64>,
    /// Reduced continuous work.
    pub port_total: f64,
    /// Reduced event transfer.
    pub event_total: f64,
}

impl CompensatedReduction {
    /// Reduces terms with Neumaier compensation while retaining every addend.
    pub fn from_transfers(
        method_id: ContentId,
        ports: &[PortWork],
        events: &[ImpulseTransfer],
    ) -> Result<Self, AuditError> {
        if ports.iter().any(|port| port.signed_work.is_nan())
            || events.iter().any(|event| event.signed_energy.is_nan())
        {
            return Err(AuditError::NonFinite("transfer"));
        }
        let port_terms = ports
            .iter()
            .map(|port| port.signed_work)
            .collect::<Vec<_>>();
        let event_terms = events
            .iter()
            .map(|event| event.signed_energy)
            .collect::<Vec<_>>();
        Ok(Self {
            port_total: neumaier(&port_terms)?,
            event_total: neumaier(&event_terms)?,
            method_id,
            port_terms,
            event_terms,
        })
    }
}

fn neumaier(values: &[f64]) -> Result<f64, AuditError> {
    let mut sum = 0.0_f64;
    let mut compensation = 0.0_f64;
    for value in values {
        if !value.is_finite() {
            return Err(AuditError::NonFinite("transfer"));
        }
        let next = sum + value;
        compensation += if sum.abs() >= value.abs() {
            (sum - next) + value
        } else {
            (value - next) + sum
        };
        sum = next;
    }
    Ok(sum + compensation)
}

macro_rules! residual_type {
    ($name:ident, $kind:literal, $quantity:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name {
            /// Signed residual magnitude in its declared quantity.
            pub value: f64,
            /// Explicit normalization scale; absent means unnormalized.
            pub normalization: Option<f64>,
            /// Whether this lane is eligible for threshold decisions.
            pub threshold_eligible: bool,
            /// Source evidence identities.
            pub evidence: Vec<ContentId>,
        }
        impl $name {
            /// Stable residual kind.
            pub const KIND: &'static str = $kind;
            /// Stable physical quantity.
            pub const QUANTITY: &'static str = $quantity;
        }
    };
}

residual_type!(
    SolverResidual,
    "solver",
    "equation-defect",
    "Residual reported by the equation solver."
);
residual_type!(
    QuadratureResidual,
    "quadrature",
    "work-integration-error",
    "Residual reported by numerical quadrature."
);
residual_type!(
    ModelResidual,
    "model",
    "model-discrepancy",
    "Residual attributed by explicit model comparison."
);
residual_type!(
    EnergyBalanceResidual,
    "energy-balance",
    "energy",
    "Observed stored-energy balance residual in joules."
);

/// All non-interchangeable residual lanes.
#[derive(Clone, Debug, PartialEq)]
pub struct ResidualLanes {
    /// Solver equation residuals.
    pub solver: Vec<SolverResidual>,
    /// Work-quadrature residuals.
    pub quadrature: Vec<QuadratureResidual>,
    /// Parameter/model residuals.
    pub model: Vec<ModelResidual>,
    /// The independently computed energy-balance observation.
    pub energy_balance: EnergyBalanceResidual,
}

/// Numerical uncertainty component.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericalUncertainty(pub UncertaintyComponent);
/// Parameter/model uncertainty component.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelUncertainty(pub UncertaintyComponent);
/// Measurement uncertainty component.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementUncertainty(pub UncertaintyComponent);

/// One signed or interval uncertainty statement with provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct UncertaintyComponent {
    /// Component identity.
    pub id: ContentId,
    /// Quantity this component concerns.
    pub quantity: String,
    /// Lower bound in the declared quantity.
    pub lower: f64,
    /// Upper bound in the declared quantity.
    pub upper: f64,
    /// Evidence for this component.
    pub evidence: Vec<ContentId>,
}

/// Explicit rule for combining uncertainty components.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CombinationRule {
    /// Keep all components separate; no aggregate is asserted.
    RetainSeparate,
    /// Conservative interval enclosure, legal only for like quantities.
    WorstCaseSameQuantity,
    /// User-supplied correlation/covariance model.
    Correlated {
        /// Identity of the correlation or covariance model.
        model_id: ContentId,
    },
}

/// Three separate uncertainty lanes and their declared combination rule.
#[derive(Clone, Debug, PartialEq)]
pub struct UncertaintyLanes {
    /// Numerical components.
    pub numerical: Vec<NumericalUncertainty>,
    /// Parameter/model components.
    pub model: Vec<ModelUncertainty>,
    /// Measurement components.
    pub measurement: Vec<MeasurementUncertainty>,
    /// Explicit combination semantics.
    pub combination: CombinationRule,
}

/// Status of discrete events within the audited span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventStatus {
    /// Whether the event inventory is declared complete.
    pub complete: bool,
    /// Evidence supporting completeness or known omission.
    pub evidence: Vec<ContentId>,
}

/// Inputs required to construct an immutable audit record.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditInput {
    /// Audited boundary.
    pub boundary: BoundaryId,
    /// Audited time span.
    pub span: WorkSpan,
    /// Independent start evaluation.
    pub start_store: StoreEvaluation,
    /// Independent end evaluation.
    pub end_store: StoreEvaluation,
    /// Per-port continuous work.
    pub port_work: Vec<PortWork>,
    /// Per-event transfer.
    pub event_transfers: Vec<ImpulseTransfer>,
    /// Declared compensated reduction identity.
    pub reduction_method: ContentId,
    /// Validated numerical-method evidence composed from the common owner.
    pub method_evidence: Vec<MethodEvidence>,
    /// Residual evidence other than the computed energy lane.
    pub solver_residuals: Vec<SolverResidual>,
    /// Quadrature residual evidence.
    pub quadrature_residuals: Vec<QuadratureResidual>,
    /// Model residual evidence.
    pub model_residuals: Vec<ModelResidual>,
    /// Separated uncertainty lanes.
    pub uncertainty: UncertaintyLanes,
    /// Event inventory status.
    pub event_status: EventStatus,
    /// Audit-level evidence references.
    pub evidence: Vec<ContentId>,
}

/// Immutable, content-identified stored-energy audit.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditRecord {
    /// Kernel content identity of the canonical audit datum.
    pub id: KernelContentId,
    /// Audited boundary.
    pub boundary: BoundaryId,
    /// Audited span.
    pub span: WorkSpan,
    /// Independent start evaluation.
    pub start_store: StoreEvaluation,
    /// Independent end evaluation.
    pub end_store: StoreEvaluation,
    /// Retained port terms.
    pub port_work: Vec<PortWork>,
    /// Retained event terms.
    pub event_transfers: Vec<ImpulseTransfer>,
    /// Declared compensated totals.
    pub reduction: CompensatedReduction,
    /// Validated common numerical-method evidence.
    pub method_evidence: Vec<MethodEvidence>,
    /// Typed residual lanes.
    pub residuals: ResidualLanes,
    /// Typed uncertainty lanes.
    pub uncertainty: UncertaintyLanes,
    /// Event inventory status.
    pub event_status: EventStatus,
    /// Audit-level evidence.
    pub evidence: Vec<ContentId>,
}

impl AuditRecord {
    /// Constructs an audit, rejecting derived endpoints and inconsistent spans/models.
    pub fn new(input: AuditInput) -> Result<Self, AuditError> {
        if input.start_store.state_id == input.end_store.state_id {
            return Err(AuditError::EndpointsNotIndependent);
        }
        if input.start_store.model_id != input.end_store.model_id {
            return Err(AuditError::ModelMismatch);
        }
        if input.span.start > input.span.end
            || input.port_work.iter().any(|work| work.span != input.span)
        {
            return Err(AuditError::SpanMismatch);
        }
        let reduction = CompensatedReduction::from_transfers(
            input.reduction_method,
            &input.port_work,
            &input.event_transfers,
        )?;
        // Visible equation: r_E = E(t1) - E(t0) - sum(port work) - sum(event transfer).
        let residual = input.end_store.energy.0
            - input.start_store.energy.0
            - reduction.port_total
            - reduction.event_total;
        if !residual.is_finite() {
            return Err(AuditError::NonFinite("energy residual"));
        }
        let energy_balance = EnergyBalanceResidual {
            value: residual,
            normalization: None,
            threshold_eligible: true,
            evidence: input.evidence.clone(),
        };
        let mut record = Self {
            id: Datum::Nil
                .content_id()
                .map_err(|_| AuditError::CanonicalIdentity)?,
            boundary: input.boundary,
            span: input.span,
            start_store: input.start_store,
            end_store: input.end_store,
            port_work: input.port_work,
            event_transfers: input.event_transfers,
            reduction,
            method_evidence: input.method_evidence,
            residuals: ResidualLanes {
                solver: input.solver_residuals,
                quadrature: input.quadrature_residuals,
                model: input.model_residuals,
                energy_balance,
            },
            uncertainty: input.uncertainty,
            event_status: input.event_status,
            evidence: input.evidence,
        };
        record.validate_uncertainty()?;
        record.id = record
            .canonical_datum()
            .content_id()
            .map_err(|_| AuditError::CanonicalIdentity)?;
        Ok(record)
    }

    /// Canonical kernel datum used for immutable identity.
    pub fn canonical_datum(&self) -> Datum {
        Datum::Node {
            tag: Symbol::qualified("physics", "energy-audit"),
            fields: vec![
                (Symbol::new("boundary"), text(self.boundary.as_str())),
                (
                    Symbol::new("span"),
                    numbers(&[self.span.start, self.span.end]),
                ),
                (
                    Symbol::new("start-store"),
                    evaluation_datum(&self.start_store),
                ),
                (Symbol::new("end-store"), evaluation_datum(&self.end_store)),
                (
                    Symbol::new("port-work"),
                    Datum::Vector(self.port_work.iter().map(port_work_datum).collect()),
                ),
                (
                    Symbol::new("event-transfer"),
                    Datum::Vector(
                        self.event_transfers
                            .iter()
                            .map(event_transfer_datum)
                            .collect(),
                    ),
                ),
                (Symbol::new("port-total"), number(self.reduction.port_total)),
                (
                    Symbol::new("event-total"),
                    number(self.reduction.event_total),
                ),
                (
                    Symbol::new("energy-residual"),
                    number(self.residuals.energy_balance.value),
                ),
                (
                    Symbol::new("solver-residuals"),
                    Datum::Vector(
                        self.residuals
                            .solver
                            .iter()
                            .map(|value| {
                                residual_datum(
                                    SolverResidual::KIND,
                                    SolverResidual::QUANTITY,
                                    value.value,
                                    value.normalization,
                                    value.threshold_eligible,
                                    &value.evidence,
                                )
                            })
                            .collect(),
                    ),
                ),
                (
                    Symbol::new("quadrature-residuals"),
                    Datum::Vector(
                        self.residuals
                            .quadrature
                            .iter()
                            .map(|value| {
                                residual_datum(
                                    QuadratureResidual::KIND,
                                    QuadratureResidual::QUANTITY,
                                    value.value,
                                    value.normalization,
                                    value.threshold_eligible,
                                    &value.evidence,
                                )
                            })
                            .collect(),
                    ),
                ),
                (
                    Symbol::new("model-residuals"),
                    Datum::Vector(
                        self.residuals
                            .model
                            .iter()
                            .map(|value| {
                                residual_datum(
                                    ModelResidual::KIND,
                                    ModelResidual::QUANTITY,
                                    value.value,
                                    value.normalization,
                                    value.threshold_eligible,
                                    &value.evidence,
                                )
                            })
                            .collect(),
                    ),
                ),
                (
                    Symbol::new("energy-balance-residual"),
                    residual_datum(
                        EnergyBalanceResidual::KIND,
                        EnergyBalanceResidual::QUANTITY,
                        self.residuals.energy_balance.value,
                        self.residuals.energy_balance.normalization,
                        self.residuals.energy_balance.threshold_eligible,
                        &self.residuals.energy_balance.evidence,
                    ),
                ),
                (
                    Symbol::new("reduction"),
                    text(self.reduction.method_id.as_str()),
                ),
                (
                    Symbol::new("method-evidence"),
                    Datum::Vector(
                        self.method_evidence
                            .iter()
                            .map(MethodCanonicalDatum::to_datum)
                            .collect(),
                    ),
                ),
                (
                    Symbol::new("events-complete"),
                    Datum::Bool(self.event_status.complete),
                ),
                (
                    Symbol::new("event-status-evidence"),
                    ids(&self.event_status.evidence),
                ),
                (
                    Symbol::new("uncertainty"),
                    uncertainty_datum(&self.uncertainty),
                ),
                (Symbol::new("evidence"), ids(&self.evidence)),
            ],
        }
    }

    /// Produces a public observed Claim about this exact immutable audit.
    pub fn claim(&self) -> Claim {
        Claim::new(
            Ref::Content(self.id.clone()),
            Symbol::qualified("physics", "energy-balance-residual"),
            Ref::Content(self.id.clone()),
        )
        .with_kind(ClaimKind::Observed)
        .with_evidence(self.evidence.iter().map(kernel_ref).collect())
    }

    fn validate_uncertainty(&self) -> Result<(), AuditError> {
        let components = self
            .uncertainty
            .numerical
            .iter()
            .map(|v| &v.0)
            .chain(self.uncertainty.model.iter().map(|v| &v.0))
            .chain(self.uncertainty.measurement.iter().map(|v| &v.0));
        for component in components {
            if !component.lower.is_finite()
                || !component.upper.is_finite()
                || component.lower > component.upper
            {
                return Err(AuditError::InvalidUncertainty);
            }
        }
        if self.uncertainty.combination == CombinationRule::WorstCaseSameQuantity {
            let mut quantities = self
                .uncertainty
                .numerical
                .iter()
                .map(|v| &v.0.quantity)
                .chain(self.uncertainty.model.iter().map(|v| &v.0.quantity))
                .chain(self.uncertainty.measurement.iter().map(|v| &v.0.quantity));
            if let Some(first) = quantities.next()
                && quantities.any(|quantity| quantity != first)
            {
                return Err(AuditError::UnlikeUncertainty);
            }
        }
        Ok(())
    }
}

/// Audit construction refusal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuditError {
    /// Endpoint states were not independently identified.
    EndpointsNotIndependent,
    /// Endpoint constitutive model identities disagree.
    ModelMismatch,
    /// A transfer uses a different or invalid span.
    SpanMismatch,
    /// A numerical input was not finite.
    NonFinite(&'static str),
    /// An uncertainty interval is malformed.
    InvalidUncertainty,
    /// Unlike quantities were combined by interval addition.
    UnlikeUncertainty,
    /// Kernel canonical identity could not be computed.
    CanonicalIdentity,
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AuditError {}

fn text(value: &str) -> Datum {
    Datum::String(value.into())
}
fn number(value: f64) -> Datum {
    Datum::Number(NumberLiteral {
        domain: Symbol::qualified("number", "f64"),
        canonical: format!("{value:.17e}"),
    })
}
fn numbers(values: &[f64]) -> Datum {
    Datum::Vector(values.iter().copied().map(number).collect())
}
fn ids(values: &[ContentId]) -> Datum {
    Datum::Vector(values.iter().map(|id| text(id.as_str())).collect())
}
fn evaluation_datum(value: &StoreEvaluation) -> Datum {
    Datum::Node {
        tag: Symbol::qualified("physics", "store-evaluation"),
        fields: vec![
            (Symbol::new("evaluator"), text(value.evaluator_id.as_str())),
            (Symbol::new("model"), text(value.model_id.as_str())),
            (Symbol::new("state"), text(value.state_id.as_str())),
            (Symbol::new("energy"), number(value.energy.0)),
            (Symbol::new("evidence"), ids(&value.evidence)),
        ],
    }
}
fn port_work_datum(value: &PortWork) -> Datum {
    Datum::Node {
        tag: Symbol::qualified("physics", "port-work"),
        fields: vec![
            (Symbol::new("boundary"), text(value.port.boundary.as_str())),
            (Symbol::new("port"), text(value.port.port.as_str())),
            (Symbol::new("pair"), text(value.port.pair.id.as_str())),
            (
                Symbol::new("effort-shape"),
                text(&value.port.pair.effort_shape),
            ),
            (Symbol::new("flow-shape"), text(&value.port.pair.flow_shape)),
            (
                Symbol::new("effort-dimension"),
                text(&format!("{:?}", value.port.pair.effort_dimension)),
            ),
            (
                Symbol::new("flow-dimension"),
                text(&format!("{:?}", value.port.pair.flow_dimension)),
            ),
            (
                Symbol::new("positive"),
                text(match value.port.positive {
                    PositiveDirection::IntoBoundary => "into-boundary",
                    PositiveDirection::OutOfBoundary => "out-of-boundary",
                }),
            ),
            (Symbol::new("source"), text(value.source.as_str())),
            (
                Symbol::new("span"),
                numbers(&[value.span.start, value.span.end]),
            ),
            (Symbol::new("work"), number(value.signed_work)),
            (Symbol::new("uncertainty"), number(value.uncertainty)),
            (
                Symbol::new("plan"),
                text(&match value.plan {
                    ReductionPlan::Trapezoid { intervals } => {
                        format!("trapezoid:{intervals}")
                    }
                    ReductionPlan::SampledTrapezoid => "sampled-trapezoid".to_owned(),
                }),
            ),
            (
                Symbol::new("segments"),
                Datum::Vector(
                    value
                        .segments
                        .iter()
                        .map(|segment| Datum::Node {
                            tag: Symbol::qualified("physics", "work-segment"),
                            fields: vec![
                                (Symbol::new("span"), numbers(&[segment.start, segment.end])),
                                (Symbol::new("method"), text(&segment.method)),
                                (
                                    Symbol::new("evaluations"),
                                    text(&segment.evaluations.to_string()),
                                ),
                                (Symbol::new("work"), number(segment.signed_work)),
                                (Symbol::new("uncertainty"), number(segment.uncertainty)),
                            ],
                        })
                        .collect(),
                ),
            ),
        ],
    }
}
fn event_transfer_datum(value: &ImpulseTransfer) -> Datum {
    Datum::Node {
        tag: Symbol::qualified("physics", "event-transfer"),
        fields: vec![
            (Symbol::new("id"), text(value.id.as_str())),
            (Symbol::new("port"), text(value.port.as_str())),
            (Symbol::new("time"), number(value.time)),
            (Symbol::new("energy"), number(value.signed_energy)),
            (
                Symbol::new("kind"),
                text(match &value.kind {
                    ImpulseKind::Mechanical => "mechanical",
                    ImpulseKind::Electrical => "electrical",
                    ImpulseKind::User(kind) => kind,
                }),
            ),
            (
                Symbol::new("state-before"),
                text(value.state_before.as_str()),
            ),
            (Symbol::new("state-after"), text(value.state_after.as_str())),
            (
                Symbol::new("constitutive-source"),
                text(value.constitutive_source.as_str()),
            ),
        ],
    }
}
fn residual_datum(
    kind: &str,
    quantity: &str,
    value: f64,
    normalization: Option<f64>,
    threshold_eligible: bool,
    evidence: &[ContentId],
) -> Datum {
    Datum::Node {
        tag: Symbol::qualified("physics", "residual"),
        fields: vec![
            (Symbol::new("kind"), text(kind)),
            (Symbol::new("quantity"), text(quantity)),
            (Symbol::new("value"), number(value)),
            (
                Symbol::new("normalization"),
                normalization.map(number).unwrap_or(Datum::Nil),
            ),
            (
                Symbol::new("threshold-eligible"),
                Datum::Bool(threshold_eligible),
            ),
            (Symbol::new("evidence"), ids(evidence)),
        ],
    }
}
fn uncertainty_datum(value: &UncertaintyLanes) -> Datum {
    fn components<'a>(values: impl Iterator<Item = &'a UncertaintyComponent>) -> Datum {
        Datum::Vector(
            values
                .map(|value| Datum::Node {
                    tag: Symbol::qualified("physics", "uncertainty-component"),
                    fields: vec![
                        (Symbol::new("id"), text(value.id.as_str())),
                        (Symbol::new("quantity"), text(&value.quantity)),
                        (
                            Symbol::new("interval"),
                            numbers(&[value.lower, value.upper]),
                        ),
                        (Symbol::new("evidence"), ids(&value.evidence)),
                    ],
                })
                .collect(),
        )
    }
    let combination = match &value.combination {
        CombinationRule::RetainSeparate => "retain-separate".to_owned(),
        CombinationRule::WorstCaseSameQuantity => "worst-case-same-quantity".to_owned(),
        CombinationRule::Correlated { model_id } => format!("correlated:{}", model_id.as_str()),
    };
    Datum::Node {
        tag: Symbol::qualified("physics", "uncertainty-lanes"),
        fields: vec![
            (
                Symbol::new("numerical"),
                components(value.numerical.iter().map(|v| &v.0)),
            ),
            (
                Symbol::new("model"),
                components(value.model.iter().map(|v| &v.0)),
            ),
            (
                Symbol::new("measurement"),
                components(value.measurement.iter().map(|v| &v.0)),
            ),
            (Symbol::new("combination"), text(&combination)),
        ],
    }
}
fn kernel_ref(value: &ContentId) -> Ref {
    Ref::Content(
        Datum::String(value.as_str().into())
            .content_id()
            .expect("string datum is canonical"),
    )
}

/// Shapes installed by the loadable audit runtime surface.
pub const SHAPES: &[&str] = &[
    "physics/StoreEvaluation",
    "physics/SolverResidual",
    "physics/QuadratureResidual",
    "physics/ModelResidual",
    "physics/EnergyBalanceResidual",
    "physics/UncertaintyLanes",
    "physics/AuditRecord",
];
/// Loadable runtime export names.
pub const RUNTIME_EXPORTS: &[&str] = &[
    "physics/evaluate-store",
    "physics/audit-energy",
    "physics/audit-claim",
];
