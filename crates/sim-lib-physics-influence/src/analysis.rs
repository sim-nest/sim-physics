use sim_incremental_core::{
    QueryBudgets, ValueFingerprint,
    dataflow::{
        AdmittedTransfer, Boundary, DataflowCompletionProof, DataflowError, DataflowGraph,
        EdgeClass, EdgeSpec, FixpointEngine, GraphBuildError, GraphDirection, JoinSemilattice,
        LawViolation, NodeSpec, StateSize, TransferPolicy,
    },
};
use sim_kernel::{
    Cx, Expr, MatchScore, Result as KernelResult, Shape, ShapeDoc, ShapeMatch, Symbol, Value,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

pub type NodeId = u64;
pub type EdgeId = u64;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InfluenceSource {
    Energy,
    Work,
    Power,
    Passivity,
    BalanceResidual,
    Derived,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum Influence {
    #[default]
    Clean,
    EnergyObserved(Vec<InfluenceSource>),
    Unknown,
}
impl Influence {
    pub fn from_source(source: InfluenceSource) -> Self {
        Self::EnergyObserved(vec![source])
    }
    pub const fn is_clean(&self) -> bool {
        matches!(self, Self::Clean)
    }
}
impl StateSize for Influence {
    fn state_size(&self) -> usize {
        match self {
            Self::Clean => 0,
            Self::EnergyObserved(v) => v.len(),
            Self::Unknown => 1,
        }
    }
}
impl JoinSemilattice for Influence {
    fn bottom(&self) -> Self {
        Self::Clean
    }
    fn join(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Clean, value) | (value, Self::Clean) => value.clone(),
            (Self::EnergyObserved(left), Self::EnergyObserved(right)) => {
                let mut sources = left.clone();
                sources.extend(right);
                sources.sort_unstable();
                sources.dedup();
                Self::EnergyObserved(sources)
            }
        }
    }
    fn less_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Clean, _) | (_, Self::Unknown) => true,
            (Self::Unknown, _) => false,
            (Self::EnergyObserved(_), Self::Clean) => false,
            (Self::EnergyObserved(left), Self::EnergyObserved(right)) => {
                left.iter().all(|v| right.contains(v))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Transform {
    Input,
    Arithmetic,
    DimensionChange,
    DomainAdapter,
    Branch,
    Join,
    Loop,
    Opaque,
    CheckedNative,
    Sink(SinkKind),
}
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SinkKind {
    Selection,
    Ranking,
    Sizing,
    Control,
}
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StudyNode {
    pub id: NodeId,
    pub location: String,
    pub transform: Transform,
}
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StudyEdge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
}

#[derive(Clone, Debug)]
pub struct StudyGraph {
    graph: DataflowGraph<NodeId, EdgeId, String, ()>,
    nodes: BTreeMap<NodeId, StudyNode>,
    declared: BTreeMap<NodeId, Influence>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StudyBuildError {
    Graph(GraphBuildError<NodeId, EdgeId>),
    DuplicateDeclaration(NodeId),
}
impl StudyGraph {
    pub fn build(
        nodes: impl IntoIterator<Item = StudyNode>,
        edges: impl IntoIterator<Item = StudyEdge>,
        inputs: impl IntoIterator<Item = (NodeId, InfluenceSource)>,
    ) -> std::result::Result<Self, StudyBuildError> {
        let nodes = nodes
            .into_iter()
            .map(|n| (n.id, n))
            .collect::<BTreeMap<_, _>>();
        let graph_nodes = nodes.values().map(|n| NodeSpec {
            id: n.id,
            location: n.location.clone(),
            boundary: if matches!(n.transform, Transform::Input) {
                Boundary::Input
            } else if matches!(n.transform, Transform::Sink(_)) {
                Boundary::Output
            } else {
                Boundary::Internal
            },
        });
        let graph_edges = edges.into_iter().map(|e| EdgeSpec {
            id: e.id,
            source: e.source,
            target: e.target,
            class: EdgeClass::Data,
            direction: GraphDirection::Forward,
        });
        let graph =
            DataflowGraph::build(graph_nodes, graph_edges).map_err(StudyBuildError::Graph)?;
        let mut declared = BTreeMap::new();
        for (id, source) in inputs {
            if declared
                .insert(id, Influence::from_source(source))
                .is_some()
            {
                return Err(StudyBuildError::DuplicateDeclaration(id));
            }
        }
        for node in nodes.values() {
            if matches!(node.transform, Transform::Opaque) {
                declared.insert(node.id, Influence::Unknown);
            }
        }
        Ok(Self {
            graph,
            nodes,
            declared,
        })
    }
    pub fn fingerprint(&self) -> ValueFingerprint {
        self.graph.fingerprint()
    }
    fn seeds(&self) -> impl Iterator<Item = (NodeId, Influence)> + '_ {
        self.declared.iter().map(|(id, state)| (*id, state.clone()))
    }
}

#[derive(Clone, Copy, Debug)]
struct PreserveInfluence;
impl TransferPolicy<Influence> for PreserveInfluence {
    fn fingerprint(&self) -> ValueFingerprint {
        ValueFingerprint::new(0x5048_5953_494e_464c)
    }
    fn policy_size(&self) -> usize {
        0
    }
    fn transfer(&self, state: &Influence) -> Influence {
        state.clone()
    }
}
type Proof = DataflowCompletionProof<NodeId, EdgeId, (), Influence>;
pub struct InfluenceAudit {
    study: StudyGraph,
    transfer: AdmittedTransfer<PreserveInfluence>,
    proof: Proof,
    budgets: QueryBudgets,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refusal {
    pub sink: NodeId,
    pub location: String,
    pub influence: Influence,
    pub path: Vec<(NodeId, Option<EdgeId>)>,
    pub path_truncated: bool,
}
#[derive(Debug)]
pub enum AuditError {
    Policy(LawViolation),
    Dataflow(DataflowError<NodeId, EdgeId, String>),
    NotSink(NodeId),
    Refused(Refusal),
}
impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AuditError {}
impl InfluenceAudit {
    pub fn complete(
        study: StudyGraph,
        budgets: QueryBudgets,
    ) -> std::result::Result<Self, AuditError> {
        let samples = [
            Influence::Clean,
            Influence::from_source(InfluenceSource::Energy),
            Influence::from_source(InfluenceSource::Work),
            Influence::Unknown,
        ];
        let transfer =
            AdmittedTransfer::admit(PreserveInfluence, &samples).map_err(AuditError::Policy)?;
        let proof = FixpointEngine::solve_proven(
            &study.graph,
            &transfer,
            Influence::Clean,
            study.seeds(),
            budgets,
        )
        .map_err(AuditError::Dataflow)?;
        Ok(Self {
            study,
            transfer,
            proof,
            budgets,
        })
    }
    pub fn complete_incremental(self, study: StudyGraph) -> std::result::Result<Self, AuditError> {
        let proof = FixpointEngine::solve_incremental(
            &self.proof,
            &study.graph,
            &self.transfer,
            Influence::Clean,
            study.seeds(),
            self.budgets,
        )
        .map_err(AuditError::Dataflow)?;
        Ok(Self {
            study,
            proof,
            ..self
        })
    }
    pub fn prepare(&self, sink: NodeId) -> std::result::Result<SelectionInput, AuditError> {
        let Some(node) = self.study.nodes.get(&sink) else {
            return Err(AuditError::NotSink(sink));
        };
        if !matches!(node.transform, Transform::Sink(_)) {
            return Err(AuditError::NotSink(sink));
        }
        let state = self
            .proof
            .solution()
            .state(&sink)
            .expect("sink belongs to proven graph");
        if !state.is_clean() {
            let mut path = Vec::new();
            let mut visited = BTreeSet::new();
            let path_truncated = collect_causes(&self.proof, sink, &mut visited, &mut path, 64);
            return Err(AuditError::Refused(Refusal {
                sink,
                location: node.location.clone(),
                influence: state.clone(),
                path,
                path_truncated,
            }));
        }
        Ok(SelectionInput {
            sink,
            proof_identity: self.proof.identity().get(),
        })
    }
    pub fn prepare_runtime(
        &self,
        cx: &mut Cx,
        request: Value,
        sink: NodeId,
    ) -> KernelResult<SelectionInput> {
        let checked = SelectionRequestShape.check_value(cx, request)?;
        if !checked.accepted {
            return Err(sim_kernel::Error::Lib(
                "physics selection request Shape rejected".into(),
            ));
        }
        self.prepare(sink)
            .map_err(|e| sim_kernel::Error::Lib(e.to_string()))
    }
    pub const fn proof_identity(&self) -> u64 {
        self.proof.identity().get()
    }
}

fn collect_causes(
    proof: &Proof,
    node: NodeId,
    visited: &mut BTreeSet<NodeId>,
    path: &mut Vec<(NodeId, Option<EdgeId>)>,
    limit: usize,
) -> bool {
    if !visited.insert(node) || path.len() >= limit {
        return path.len() >= limit;
    }
    let Some(explanation) = proof
        .solution()
        .explain(&node, limit.saturating_sub(path.len()))
    else {
        return false;
    };
    let mut truncated = explanation.truncated();
    for cause in explanation.predecessors() {
        if path.len() >= limit {
            return true;
        }
        path.push((cause.node, cause.edge));
        truncated |= collect_causes(proof, cause.node, visited, path, limit);
    }
    truncated
}
#[derive(Debug)]
pub struct SelectionInput {
    sink: NodeId,
    proof_identity: u64,
}
impl SelectionInput {
    pub const fn sink(&self) -> NodeId {
        self.sink
    }
    pub const fn proof_identity(&self) -> u64 {
        self.proof_identity
    }
}
pub trait CleanSelection {
    fn selection_input(&self) -> &SelectionInput;
}
impl CleanSelection for SelectionInput {
    fn selection_input(&self) -> &SelectionInput {
        self
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SelectionRequestShape;
impl Shape for SelectionRequestShape {
    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol::qualified("physics/influence", "SelectionRequest"))
    }
    fn check_value(&self, cx: &mut Cx, value: Value) -> KernelResult<ShapeMatch> {
        let expr = value.object().as_expr(cx)?;
        self.check_expr(cx, &expr)
    }
    fn check_expr(&self, _cx: &mut Cx, expr: &Expr) -> KernelResult<ShapeMatch> {
        match expr {
            Expr::List(items) if items.len() == 2 => Ok(ShapeMatch::accept(MatchScore::exact(100))),
            _ => Ok(ShapeMatch::reject(
                "physics selection request must be (sink candidate)",
            )),
        }
    }
    fn describe(&self, _cx: &mut Cx) -> KernelResult<ShapeDoc> {
        Ok(ShapeDoc::new("proof-prepared physics selection request").with_detail("Shape acceptance is necessary but cannot replace a clean dataflow completion proof"))
    }
}
