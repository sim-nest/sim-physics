use super::*;

pub(super) fn text(value: &str) -> Datum {
    Datum::String(value.into())
}
pub(super) fn number(value: f64) -> Datum {
    Datum::Number(NumberLiteral {
        domain: Symbol::qualified("number", "f64"),
        canonical: format!("{value:.17e}"),
    })
}
pub(super) fn numbers(values: &[f64]) -> Datum {
    Datum::Vector(values.iter().copied().map(number).collect())
}
pub(super) fn ids(values: &[ContentId]) -> Datum {
    Datum::Vector(values.iter().map(|id| text(id.as_str())).collect())
}
pub(super) fn evaluation_datum(value: &StoreEvaluation) -> Datum {
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
pub(super) fn port_work_datum(value: &PortWork) -> Datum {
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
pub(super) fn event_transfer_datum(value: &ImpulseTransfer) -> Datum {
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
pub(super) fn residual_datum(
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
pub(super) fn uncertainty_datum(value: &UncertaintyLanes) -> Datum {
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
pub(super) fn kernel_ref(value: &ContentId) -> Ref {
    Ref::Content(
        Datum::String(value.as_str().into())
            .content_id()
            .expect("string datum is canonical"),
    )
}
