use super::*;

pub(super) fn status_text(v: Status) -> &'static str {
    match v {
        Status::Open => "physics/finding/open",
        Status::Explained => "physics/finding/explained",
        Status::Confirmed => "physics/finding/confirmed",
        Status::Reclassified => "physics/finding/reclassified",
        Status::Resolved => "physics/finding/resolved",
        Status::Withdrawn => "physics/finding/withdrawn",
    }
}
pub(super) fn id_text(id: &FindingId) -> String {
    id.bytes.iter().map(|v| format!("{v:02x}")).collect()
}
pub(super) fn encode_heads(heads: &BTreeSet<FindingId>) -> Vec<u8> {
    heads
        .iter()
        .map(id_text)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

pub(super) fn put(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}
pub(super) fn put_str(out: &mut Vec<u8>, value: &str) {
    put(out, value.as_bytes());
}
pub(super) fn put_id(out: &mut Vec<u8>, id: &FindingId) {
    put_str(out, &id.algorithm.name);
    out.extend_from_slice(&id.bytes);
}
pub(super) fn encode(r: &FindingRecord) -> Result<Vec<u8>, FindingError> {
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
pub(super) fn r_sign(v: Sign) -> u8 {
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
pub(super) fn decode(bytes: &[u8]) -> Result<FindingRecord, FindingError> {
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
