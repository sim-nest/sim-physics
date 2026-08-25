#![forbid(unsafe_code)]
//! Immutable, branch-preserving histories for physics findings.

use sha2::{Digest, Sha256};
use sim_kernel::{Claim, ClaimKind, ContentId, Ref, Symbol};
use sim_storage_port::{HostDirErrorKind, HostDirPort, NeverCancel};
use sim_table_core::{TablePath, is_legal_table_segment};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

/// Stable identity of an immutable finding record.
pub type FindingId = ContentId;

/// Observation sign, kept separate from magnitude.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sign {
    /// Positive residual.
    Positive,
    /// Negative residual.
    Negative,
    /// Exactly zero.
    Zero,
}

/// Lifecycle status. Successors may advance but never mutate earlier records.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Status {
    /// Investigation is active.
    Open,
    /// Evidence explains the observation.
    Explained,
    /// Independent evidence confirms it.
    Confirmed,
    /// The interpretation changed.
    Reclassified,
    /// The issue is closed with a substantive reason.
    Resolved,
    /// The author retracts the interpretation without erasing it.
    Withdrawn,
}

/// Typed relation from a successor to one or more predecessors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuccessorKind {
    /// Supplies an explanation.
    Explained,
    /// Confirms prior work.
    Confirmed,
    /// Changes classification.
    Reclassified,
    /// Reconciles and closes predecessors.
    Resolved,
    /// Withdraws a conclusion.
    Withdrawn,
}

/// One public evidence reference.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EvidenceRef {
    /// Validated evidence scheme.
    pub scheme: String,
    /// Non-secret opaque identifier.
    pub id: String,
}

/// One named uncertainty lane.
#[derive(Clone, Debug, PartialEq)]
pub struct UncertaintyLane {
    /// Lane name.
    pub name: String,
    /// Non-negative finite absolute uncertainty.
    pub absolute: f64,
}

/// Immutable observation payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Non-negative finite magnitude.
    pub magnitude: f64,
    /// Sign of the residual.
    pub sign: Sign,
    /// Content identity of the boundary.
    pub boundary_id: FindingId,
    /// Content identity of the study.
    pub study_id: FindingId,
    /// Human-reviewable occurrence conditions.
    pub occurrence_conditions: String,
    /// Stable residual classification.
    pub residual_class: String,
    /// Model identity used by projections.
    pub model: String,
    /// Stable parameter-region description.
    pub parameter_region: String,
    /// Separated uncertainty lanes.
    pub uncertainty: Vec<UncertaintyLane>,
    /// Checks actually performed.
    pub checks_performed: Vec<String>,
}

/// Immutable finding or successor record.
#[derive(Clone, Debug, PartialEq)]
pub struct FindingRecord {
    /// Observation is repeated in successors so every record is independently inspectable.
    pub observation: Observation,
    /// Typed successor relation; absent only for a root observation.
    pub successor: Option<SuccessorKind>,
    /// Predecessors cited by a successor.
    pub predecessors: Vec<FindingId>,
    /// New evidence introduced by this record.
    pub evidence: Vec<EvidenceRef>,
    /// Auditable authoring method, never a private author identity.
    pub authoring_method: String,
    /// Lifecycle status represented by this immutable record.
    pub status: Status,
    /// Substantive explanation for the transition.
    pub rationale: String,
    /// Private author note, deliberately excluded from durable encoding and projections.
    pub private_note: Option<String>,
}

impl FindingRecord {
    /// Constructs an open root observation.
    pub fn open(
        observation: Observation,
        evidence: Vec<EvidenceRef>,
        method: impl Into<String>,
    ) -> Self {
        Self {
            observation,
            successor: None,
            predecessors: Vec::new(),
            evidence,
            authoring_method: method.into(),
            status: Status::Open,
            rationale: "initial observation".into(),
            private_note: None,
        }
    }

    /// Constructs a typed successor. Repository validation checks its ancestry.
    pub fn successor(
        observation: Observation,
        kind: SuccessorKind,
        predecessors: Vec<FindingId>,
        evidence: Vec<EvidenceRef>,
        method: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        let status = match kind {
            SuccessorKind::Explained => Status::Explained,
            SuccessorKind::Confirmed => Status::Confirmed,
            SuccessorKind::Reclassified => Status::Reclassified,
            SuccessorKind::Resolved => Status::Resolved,
            SuccessorKind::Withdrawn => Status::Withdrawn,
        };
        Self {
            observation,
            successor: Some(kind),
            predecessors,
            evidence,
            authoring_method: method.into(),
            status,
            rationale: rationale.into(),
            private_note: None,
        }
    }

    /// Returns canonical public bytes. Private notes can never affect identity or storage.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FindingError> {
        encode(self)
    }

    /// Returns the stable content identity of canonical public bytes.
    pub fn content_id(&self) -> Result<FindingId, FindingError> {
        let bytes: [u8; 32] = Sha256::digest(self.canonical_bytes()?).into();
        Ok(ContentId::from_bytes(Symbol::new("core/sha256"), bytes))
    }

    /// Produces a public observed Claim about this exact immutable record.
    pub fn claim(&self) -> Result<Claim, FindingError> {
        let id = self.content_id()?;
        Ok(Claim::new(
            Ref::Content(id),
            Symbol::new("physics/finding-status"),
            Ref::Symbol(Symbol::new(status_text(self.status))),
        )
        .with_kind(ClaimKind::Observed))
    }
}

/// Query over disposable projections rebuilt from authoritative records.
#[derive(Clone, Debug, Default)]
pub struct FindingQuery {
    /// Required status.
    pub status: Option<Status>,
    /// Required residual class.
    pub residual_class: Option<String>,
    /// Required model.
    pub model: Option<String>,
    /// Required parameter region.
    pub parameter_region: Option<String>,
    /// Inclusive magnitude range.
    pub magnitude: Option<(f64, f64)>,
    /// Record whose evidence chain must contain this id.
    pub evidence_chain: Option<FindingId>,
}

/// Fully rebuilt, read-only projection.
#[derive(Clone, Debug, Default)]
pub struct Projection {
    records: BTreeMap<FindingId, FindingRecord>,
    heads: BTreeSet<FindingId>,
}

impl Projection {
    /// Returns current branch heads.
    pub fn heads(&self) -> &BTreeSet<FindingId> {
        &self.heads
    }
    /// Returns a record by content id.
    pub fn get(&self, id: &FindingId) -> Option<&FindingRecord> {
        self.records.get(id)
    }
    /// Selects records without exposing private notes.
    pub fn query(&self, query: &FindingQuery) -> Vec<(FindingId, FindingRecord)> {
        self.records
            .iter()
            .filter(|(id, record)| {
                query.status.is_none_or(|v| record.status == v)
                    && query
                        .residual_class
                        .as_ref()
                        .is_none_or(|v| &record.observation.residual_class == v)
                    && query
                        .model
                        .as_ref()
                        .is_none_or(|v| &record.observation.model == v)
                    && query
                        .parameter_region
                        .as_ref()
                        .is_none_or(|v| &record.observation.parameter_region == v)
                    && query.magnitude.is_none_or(|(lo, hi)| {
                        record.observation.magnitude >= lo && record.observation.magnitude <= hi
                    })
                    && query.evidence_chain.as_ref().is_none_or(|ancestor| {
                        id == &ancestor || reaches(&self.records, id, ancestor)
                    })
            })
            .map(|(id, record)| {
                let mut public = record.clone();
                public.private_note = None;
                (id.clone(), public)
            })
            .collect()
    }
}

/// Repository failure, sanitized from native paths and handles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FindingError {
    /// Record fields or transition are invalid.
    Malformed(String),
    /// A predecessor does not exist.
    MissingPredecessor,
    /// Stored bytes do not match their content id or schema.
    CorruptRecord,
    /// A successor would create a cycle.
    Cycle,
    /// Concurrent publication changed the head set; caller may rebuild and retry.
    ConcurrentAppend,
    /// Injected backend failure by stable category only.
    Storage(HostDirErrorKind),
}

impl fmt::Display for FindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FindingError {}

/// Append-only repository over an injected portable Table/Dir backend.
pub struct FindingRepository {
    port: Arc<dyn HostDirPort>,
    root: TablePath,
}

impl FindingRepository {
    /// Opens a repository below a validated Table path.
    pub fn new(port: Arc<dyn HostDirPort>, root: &[&str]) -> Result<Self, FindingError> {
        let mut path = TablePath::root();
        for segment in root {
            path.push(segment)
                .map_err(|_| FindingError::Malformed("invalid repository path".into()))?;
        }
        Ok(Self { port, root: path })
    }

    /// Initializes disposable directory structure without changing history.
    pub fn initialize(&self) -> Result<(), FindingError> {
        self.port.create_dir(&self.path(&[])).map_err(storage)?;
        self.port
            .create_dir(&self.path(&["records"]))
            .map_err(storage)
    }

    /// Validates and atomically appends a record and new branch-head set.
    pub fn append(&self, record: &FindingRecord) -> Result<FindingId, FindingError> {
        validate_fields(record)?;
        let projection = self.rebuild()?;
        validate_transition(record, &projection.records)?;
        let id = record.content_id()?;
        let bytes = record.canonical_bytes()?;
        let record_path = self.record_path(&id);
        match self.port.metadata(&record_path).map_err(storage)? {
            Some(_) => {
                if self.port.read(&record_path).map_err(storage)? != bytes {
                    return Err(FindingError::CorruptRecord);
                }
            }
            None => self
                .port
                .compare_exchange(&record_path, None, Some(&bytes), &NeverCancel)
                .map_err(storage)
                .and_then(|out| {
                    if out.exchanged {
                        Ok(())
                    } else {
                        Err(FindingError::ConcurrentAppend)
                    }
                })?,
        }
        let observed = self.read_heads_bytes()?;
        let mut heads = projection.heads;
        for predecessor in &record.predecessors {
            heads.remove(predecessor);
        }
        heads.insert(id.clone());
        let replacement = encode_heads(&heads);
        let result = self
            .port
            .compare_exchange(
                &self.path(&["heads"]),
                observed.as_deref(),
                Some(&replacement),
                &NeverCancel,
            )
            .map_err(storage)?;
        if !result.exchanged {
            return Err(FindingError::ConcurrentAppend);
        }
        Ok(id)
    }

    /// Rebuilds all projections solely from immutable content-addressed records.
    pub fn rebuild(&self) -> Result<Projection, FindingError> {
        let mut records = BTreeMap::new();
        let dir = self.path(&["records"]);
        let entries = match self.port.list(&dir) {
            Ok(v) => v,
            Err(e) if e.kind == HostDirErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(storage(e)),
        };
        for entry in entries {
            if !is_legal_table_segment(&entry.name) {
                return Err(FindingError::CorruptRecord);
            }
            let bytes = self
                .port
                .read(&self.path(&["records", &entry.name]))
                .map_err(storage)?;
            let record = decode(&bytes)?;
            let id = record.content_id()?;
            if id_text(&id) != entry.name {
                return Err(FindingError::CorruptRecord);
            }
            records.insert(id, record);
        }
        for record in records.values() {
            validate_transition(record, &records)?;
        }
        let mut heads: BTreeSet<_> = records.keys().cloned().collect();
        for record in records.values() {
            for predecessor in &record.predecessors {
                heads.remove(predecessor);
            }
        }
        Ok(Projection { records, heads })
    }

    fn path(&self, tail: &[&str]) -> Vec<String> {
        self.root
            .segments()
            .iter()
            .cloned()
            .chain(tail.iter().map(|v| (*v).to_owned()))
            .collect()
    }
    fn record_path(&self, id: &FindingId) -> Vec<String> {
        self.path(&["records", &id_text(id)])
    }
    fn read_heads_bytes(&self) -> Result<Option<Vec<u8>>, FindingError> {
        match self.port.read(&self.path(&["heads"])) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind == HostDirErrorKind::NotFound => Ok(None),
            Err(e) => Err(storage(e)),
        }
    }
}

fn validate_fields(r: &FindingRecord) -> Result<(), FindingError> {
    let o = &r.observation;
    if !o.magnitude.is_finite()
        || o.magnitude < 0.0
        || o.occurrence_conditions.trim().is_empty()
        || o.residual_class.trim().is_empty()
        || o.model.trim().is_empty()
        || o.parameter_region.trim().is_empty()
        || r.authoring_method.trim().is_empty()
        || r.rationale.trim().is_empty()
        || r.evidence.is_empty()
        || o.checks_performed.is_empty()
        || o.checks_performed.iter().any(|v| v.trim().is_empty())
        || o.uncertainty.is_empty()
        || o.uncertainty
            .iter()
            .any(|v| v.name.trim().is_empty() || !v.absolute.is_finite() || v.absolute < 0.0)
        || r.evidence.iter().any(|v| {
            !matches!(
                v.scheme.as_str(),
                "content" | "claim" | "study" | "measurement"
            ) || v.id.trim().is_empty()
        })
    {
        return Err(FindingError::Malformed("finding fields or evidence".into()));
    }
    if (o.magnitude == 0.0) != (o.sign == Sign::Zero) {
        return Err(FindingError::Malformed("magnitude/sign mismatch".into()));
    }
    if r.successor.is_none() != r.predecessors.is_empty() {
        return Err(FindingError::Malformed("root/successor shape".into()));
    }
    Ok(())
}

fn validate_transition(
    r: &FindingRecord,
    records: &BTreeMap<FindingId, FindingRecord>,
) -> Result<(), FindingError> {
    if r.successor.is_none() {
        return Ok(());
    }
    if r.predecessors.iter().collect::<BTreeSet<_>>().len() != r.predecessors.len() {
        return Err(FindingError::Malformed("duplicate predecessor".into()));
    }
    let id = r.content_id()?;
    for predecessor in &r.predecessors {
        let Some(prior) = records.get(predecessor) else {
            return Err(FindingError::MissingPredecessor);
        };
        if predecessor == &id || reaches(records, predecessor, &id) {
            return Err(FindingError::Cycle);
        }
        if r.status < prior.status && !r.rationale.to_ascii_lowercase().contains("reclass") {
            return Err(FindingError::Malformed(
                "status rollback requires reclassification explanation".into(),
            ));
        }
    }
    if r.successor == Some(SuccessorKind::Resolved) {
        let reason = r.rationale.to_ascii_lowercase();
        if reason.contains("inconvenien")
            || reason.contains("threshold")
            || r.predecessors.len() < 2 && reason.len() < 24
        {
            return Err(FindingError::Malformed(
                "resolution requires substantive reconciliation evidence".into(),
            ));
        }
    }
    Ok(())
}

fn reaches(
    records: &BTreeMap<FindingId, FindingRecord>,
    start: &FindingId,
    target: &FindingId,
) -> bool {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if id == target {
            return true;
        }
        if seen.insert(id.clone())
            && let Some(r) = records.get(id)
        {
            pending.extend(&r.predecessors);
        }
    }
    false
}

fn storage(e: sim_storage_port::HostDirError) -> FindingError {
    FindingError::Storage(e.kind)
}
fn status_text(v: Status) -> &'static str {
    match v {
        Status::Open => "physics/finding/open",
        Status::Explained => "physics/finding/explained",
        Status::Confirmed => "physics/finding/confirmed",
        Status::Reclassified => "physics/finding/reclassified",
        Status::Resolved => "physics/finding/resolved",
        Status::Withdrawn => "physics/finding/withdrawn",
    }
}
fn id_text(id: &FindingId) -> String {
    id.bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn encode_heads(heads: &BTreeSet<FindingId>) -> Vec<u8> {
    heads
        .iter()
        .map(id_text)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

fn put(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}
fn put_str(out: &mut Vec<u8>, value: &str) {
    put(out, value.as_bytes());
}
fn put_id(out: &mut Vec<u8>, id: &FindingId) {
    put_str(out, &id.algorithm.name);
    out.extend_from_slice(&id.bytes);
}
fn encode(r: &FindingRecord) -> Result<Vec<u8>, FindingError> {
    validate_fields(r)?;
    let mut out = b"sim.physics.finding/v1".to_vec();
    let o = &r.observation;
    out.extend_from_slice(&o.magnitude.to_bits().to_be_bytes());
    out.push(r_sign(o.sign));
    put_id(&mut out, &o.boundary_id);
    put_id(&mut out, &o.study_id);
    for v in [
        &o.occurrence_conditions,
        &o.residual_class,
        &o.model,
        &o.parameter_region,
    ] {
        put_str(&mut out, v);
    }
    out.extend_from_slice(&(o.uncertainty.len() as u64).to_be_bytes());
    for v in &o.uncertainty {
        put_str(&mut out, &v.name);
        out.extend_from_slice(&v.absolute.to_bits().to_be_bytes());
    }
    out.extend_from_slice(&(o.checks_performed.len() as u64).to_be_bytes());
    for v in &o.checks_performed {
        put_str(&mut out, v);
    }
    out.push(r.successor.map_or(0, |v| v as u8 + 1));
    out.extend_from_slice(&(r.predecessors.len() as u64).to_be_bytes());
    for v in &r.predecessors {
        put_id(&mut out, v);
    }
    out.extend_from_slice(&(r.evidence.len() as u64).to_be_bytes());
    for v in &r.evidence {
        put_str(&mut out, &v.scheme);
        put_str(&mut out, &v.id);
    }
    put_str(&mut out, &r.authoring_method);
    out.push(r.status as u8);
    put_str(&mut out, &r.rationale);
    Ok(out)
}
fn r_sign(v: Sign) -> u8 {
    match v {
        Sign::Positive => 0,
        Sign::Negative => 1,
        Sign::Zero => 2,
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FindingError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|v| *v <= self.bytes.len())
            .ok_or(FindingError::CorruptRecord)?;
        let v = &self.bytes[self.at..end];
        self.at = end;
        Ok(v)
    }
    fn u64(&mut self) -> Result<u64, FindingError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| FindingError::CorruptRecord)?,
        ))
    }
    fn string(&mut self) -> Result<String, FindingError> {
        let n = usize::try_from(self.u64()?).map_err(|_| FindingError::CorruptRecord)?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| FindingError::CorruptRecord)
    }
    fn id(&mut self) -> Result<FindingId, FindingError> {
        let a = self.string()?;
        let b: [u8; 32] = self
            .take(32)?
            .try_into()
            .map_err(|_| FindingError::CorruptRecord)?;
        Ok(ContentId::from_bytes(Symbol::new(a), b))
    }
}
fn decode(bytes: &[u8]) -> Result<FindingRecord, FindingError> {
    let mut d = Reader { bytes, at: 0 };
    if d.take(22)? != b"sim.physics.finding/v1" {
        return Err(FindingError::CorruptRecord);
    }
    let magnitude = f64::from_bits(d.u64()?);
    let sign = match d.take(1)?[0] {
        0 => Sign::Positive,
        1 => Sign::Negative,
        2 => Sign::Zero,
        _ => return Err(FindingError::CorruptRecord),
    };
    let boundary_id = d.id()?;
    let study_id = d.id()?;
    let occurrence_conditions = d.string()?;
    let residual_class = d.string()?;
    let model = d.string()?;
    let parameter_region = d.string()?;
    let mut uncertainty = Vec::new();
    for _ in 0..d.u64()? {
        uncertainty.push(UncertaintyLane {
            name: d.string()?,
            absolute: f64::from_bits(d.u64()?),
        })
    }
    let mut checks_performed = Vec::new();
    for _ in 0..d.u64()? {
        checks_performed.push(d.string()?)
    }
    let successor = match d.take(1)?[0] {
        0 => None,
        1 => Some(SuccessorKind::Explained),
        2 => Some(SuccessorKind::Confirmed),
        3 => Some(SuccessorKind::Reclassified),
        4 => Some(SuccessorKind::Resolved),
        5 => Some(SuccessorKind::Withdrawn),
        _ => return Err(FindingError::CorruptRecord),
    };
    let mut predecessors = Vec::new();
    for _ in 0..d.u64()? {
        predecessors.push(d.id()?)
    }
    let mut evidence = Vec::new();
    for _ in 0..d.u64()? {
        evidence.push(EvidenceRef {
            scheme: d.string()?,
            id: d.string()?,
        })
    }
    let authoring_method = d.string()?;
    let status = match d.take(1)?[0] {
        0 => Status::Open,
        1 => Status::Explained,
        2 => Status::Confirmed,
        3 => Status::Reclassified,
        4 => Status::Resolved,
        5 => Status::Withdrawn,
        _ => return Err(FindingError::CorruptRecord),
    };
    let rationale = d.string()?;
    if d.at != bytes.len() {
        return Err(FindingError::CorruptRecord);
    }
    let r = FindingRecord {
        observation: Observation {
            magnitude,
            sign,
            boundary_id,
            study_id,
            occurrence_conditions,
            residual_class,
            model,
            parameter_region,
            uncertainty,
            checks_performed,
        },
        successor,
        predecessors,
        evidence,
        authoring_method,
        status,
        rationale,
        private_note: None,
    };
    validate_fields(&r)?;
    Ok(r)
}

/// Runtime export symbols supplied by the loadable library surface.
pub const RUNTIME_EXPORTS: &[&str] = &[
    "physics/finding/open",
    "physics/finding/succeed",
    "physics/finding/query",
    "physics/finding/rebuild",
];
