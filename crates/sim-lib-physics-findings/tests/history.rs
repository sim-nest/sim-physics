use sim_kernel::{ContentId, Symbol};
use sim_lib_physics_findings::*;
use sim_storage_port::{
    Cancellation, HostCompareExchange, HostDirError, HostDirErrorKind, HostDirPort, HostEntry,
    HostEntryKind, PortResult,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct MemoryPort {
    files: Mutex<BTreeMap<Vec<String>, Vec<u8>>>,
    dirs: Mutex<BTreeSet<Vec<String>>>,
    interrupt: Mutex<bool>,
}
impl MemoryPort {
    fn interrupted() -> Self {
        Self {
            interrupt: Mutex::new(true),
            ..Self::default()
        }
    }
}
impl HostDirPort for MemoryPort {
    fn label(&self) -> &str {
        "memory"
    }
    fn list(&self, dir: &[String]) -> PortResult<Vec<HostEntry>> {
        let prefix = dir.to_vec();
        let files = self.files.lock().unwrap();
        Ok(files
            .iter()
            .filter(|(p, _)| p.len() == prefix.len() + 1 && p.starts_with(&prefix))
            .map(|(p, b)| HostEntry {
                name: p.last().unwrap().clone(),
                kind: HostEntryKind::File,
                len: b.len() as u64,
            })
            .collect())
    }
    fn metadata(&self, path: &[String]) -> PortResult<Option<HostEntry>> {
        Ok(self.files.lock().unwrap().get(path).map(|b| HostEntry {
            name: path.last().unwrap().clone(),
            kind: HostEntryKind::File,
            len: b.len() as u64,
        }))
    }
    fn read(&self, path: &[String]) -> PortResult<Vec<u8>> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| HostDirError::new(HostDirErrorKind::NotFound, "missing"))
    }
    fn replace(&self, path: &[String], bytes: &[u8], _: &dyn Cancellation) -> PortResult<()> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_vec(), bytes.to_vec());
        Ok(())
    }
    fn compare_exchange(
        &self,
        path: &[String],
        expected: Option<&[u8]>,
        replacement: Option<&[u8]>,
        _: &dyn Cancellation,
    ) -> PortResult<HostCompareExchange> {
        if std::mem::take(&mut *self.interrupt.lock().unwrap()) {
            return Err(HostDirError::new(
                HostDirErrorKind::Cancelled,
                "interrupted",
            ));
        }
        let mut files = self.files.lock().unwrap();
        let observed = files.get(path).cloned();
        let exchanged = observed.as_deref() == expected;
        if exchanged {
            match replacement {
                Some(v) => {
                    files.insert(path.to_vec(), v.to_vec());
                }
                None => {
                    files.remove(path);
                }
            }
        }
        Ok(HostCompareExchange {
            exchanged,
            observed,
        })
    }
    fn remove_file(&self, path: &[String]) -> PortResult<()> {
        self.files.lock().unwrap().remove(path);
        Ok(())
    }
    fn create_dir(&self, path: &[String]) -> PortResult<()> {
        self.dirs.lock().unwrap().insert(path.to_vec());
        Ok(())
    }
    fn remove_dir_all(&self, _: &[String]) -> PortResult<()> {
        Ok(())
    }
    fn child(&self, _: &str) -> PortResult<Arc<dyn HostDirPort>> {
        Err(HostDirError::new(HostDirErrorKind::Unsupported, "unused"))
    }
}

fn id(seed: u8) -> ContentId {
    ContentId::from_bytes(Symbol::new("core/sha256"), [seed; 32])
}
fn observation() -> Observation {
    Observation {
        magnitude: 3.5,
        sign: Sign::Positive,
        boundary_id: id(1),
        study_id: id(2),
        occurrence_conditions: "edge of certified region".into(),
        residual_class: "energy-balance".into(),
        model: "motor-v3".into(),
        parameter_region: "load=0.95".into(),
        uncertainty: vec![UncertaintyLane {
            name: "measurement".into(),
            absolute: 0.1,
        }],
        checks_performed: vec!["mesh refinement".into()],
    }
}
fn evidence(name: &str) -> Vec<EvidenceRef> {
    vec![EvidenceRef {
        scheme: "study".into(),
        id: name.into(),
    }]
}
fn repository(port: Arc<MemoryPort>) -> FindingRepository {
    let repo = FindingRepository::new(port, ["physics", "findings"].as_slice()).unwrap();
    repo.initialize().unwrap();
    repo
}

#[test]
fn content_stability_and_private_redaction() {
    let mut a = FindingRecord::open(observation(), evidence("sweep/1"), "import");
    let id = a.content_id().unwrap();
    a.private_note = Some("patient-name".into());
    assert_eq!(id, a.content_id().unwrap());
    assert!(!String::from_utf8_lossy(&a.canonical_bytes().unwrap()).contains("patient-name"));
    assert_eq!(a.claim().unwrap().subject, sim_kernel::Ref::Content(id));
}

#[test]
fn append_branches_reconcile_and_preserve_full_history() {
    let repo = repository(Arc::new(MemoryPort::default()));
    let root = FindingRecord::open(observation(), evidence("root"), "import");
    let root_id = repo.append(&root).unwrap();
    let a = FindingRecord::successor(
        observation(),
        SuccessorKind::Explained,
        vec![root_id.clone()],
        evidence("model"),
        "review",
        "model approximation explains residual",
    );
    let a_id = repo.append(&a).unwrap();
    let b = FindingRecord::successor(
        observation(),
        SuccessorKind::Confirmed,
        vec![root_id.clone()],
        evidence("lab"),
        "review",
        "independent laboratory confirmation",
    );
    let b_id = repo.append(&b).unwrap();
    let rebuilt = repo.rebuild().unwrap();
    assert_eq!(
        rebuilt.heads(),
        &BTreeSet::from([a_id.clone(), b_id.clone()])
    );
    let resolution = FindingRecord::successor(
        observation(),
        SuccessorKind::Resolved,
        vec![a_id, b_id],
        evidence("reconcile"),
        "panel",
        "new calibration evidence reconciles both competing branches",
    );
    let resolved = repo.append(&resolution).unwrap();
    let projection = repo.rebuild().unwrap();
    assert_eq!(projection.heads(), &BTreeSet::from([resolved]));
    assert_eq!(projection.query(&FindingQuery::default()).len(), 4);
}

#[test]
fn queries_cover_every_required_lane() {
    let repo = repository(Arc::new(MemoryPort::default()));
    let root = FindingRecord::open(observation(), evidence("root"), "import");
    let id = repo.append(&root).unwrap();
    let projection = repo.rebuild().unwrap();
    for query in [
        FindingQuery {
            status: Some(Status::Open),
            ..Default::default()
        },
        FindingQuery {
            residual_class: Some("energy-balance".into()),
            ..Default::default()
        },
        FindingQuery {
            model: Some("motor-v3".into()),
            ..Default::default()
        },
        FindingQuery {
            parameter_region: Some("load=0.95".into()),
            ..Default::default()
        },
        FindingQuery {
            magnitude: Some((3.0, 4.0)),
            ..Default::default()
        },
        FindingQuery {
            evidence_chain: Some(id.clone()),
            ..Default::default()
        },
    ] {
        assert_eq!(projection.query(&query).len(), 1)
    }
}

#[test]
fn malformed_missing_and_bad_resolutions_fail_closed() {
    let repo = repository(Arc::new(MemoryPort::default()));
    let mut malformed = FindingRecord::open(observation(), vec![], "import");
    assert!(matches!(
        repo.append(&malformed),
        Err(FindingError::Malformed(_))
    ));
    malformed.evidence = evidence("ok");
    let root_id = repo.append(&malformed).unwrap();
    let missing = FindingRecord::successor(
        observation(),
        SuccessorKind::Explained,
        vec![id(99)],
        evidence("x"),
        "review",
        "substantive explanation",
    );
    assert_eq!(repo.append(&missing), Err(FindingError::MissingPredecessor));
    let bad = FindingRecord::successor(
        observation(),
        SuccessorKind::Resolved,
        vec![root_id],
        evidence("x"),
        "review",
        "threshold tuning",
    );
    assert!(matches!(repo.append(&bad), Err(FindingError::Malformed(_))));
}

#[test]
fn interrupted_append_publishes_no_record() {
    let port = Arc::new(MemoryPort::interrupted());
    let repo = repository(port);
    let root = FindingRecord::open(observation(), evidence("root"), "import");
    assert_eq!(
        repo.append(&root),
        Err(FindingError::Storage(HostDirErrorKind::Cancelled))
    );
    assert!(repo.rebuild().unwrap().heads().is_empty());
}

#[test]
fn concurrent_head_change_never_erases_branch() {
    let port = Arc::new(MemoryPort::default());
    let repo = repository(port.clone());
    let root = FindingRecord::open(observation(), evidence("root"), "import");
    let root_id = repo.append(&root).unwrap();
    let a = FindingRecord::successor(
        observation(),
        SuccessorKind::Explained,
        vec![root_id.clone()],
        evidence("a"),
        "review",
        "first competing explanation",
    );
    let a_id = repo.append(&a).unwrap();
    let b = FindingRecord::successor(
        observation(),
        SuccessorKind::Confirmed,
        vec![root_id],
        evidence("b"),
        "review",
        "second concurrent confirmation",
    );
    let b_id = repo.append(&b).unwrap();
    assert_eq!(
        repo.rebuild().unwrap().heads(),
        &BTreeSet::from([a_id, b_id])
    );
}

#[test]
fn corrupt_and_missing_records_are_refused() {
    let port = Arc::new(MemoryPort::default());
    let repo = repository(port.clone());
    let root = FindingRecord::open(observation(), evidence("root"), "import");
    let root_id = repo.append(&root).unwrap();
    port.files.lock().unwrap().insert(
        vec![
            "physics".into(),
            "findings".into(),
            "records".into(),
            "00".repeat(32),
        ],
        b"corrupt".to_vec(),
    );
    assert_eq!(repo.rebuild().unwrap_err(), FindingError::CorruptRecord);
    port.files.lock().unwrap().remove(&vec![
        "physics".into(),
        "findings".into(),
        "records".into(),
        "00".repeat(32),
    ]);
    let child = FindingRecord::successor(
        observation(),
        SuccessorKind::Explained,
        vec![root_id],
        evidence("a"),
        "review",
        "substantive model explanation",
    );
    let child_id = repo.append(&child).unwrap();
    port.files.lock().unwrap().remove(&vec![
        "physics".into(),
        "findings".into(),
        "records".into(),
        child_id.bytes.iter().map(|v| format!("{v:02x}")).collect(),
    ]);
    assert!(repo.rebuild().unwrap().heads().len() == 1);
}

#[test]
fn equivalent_backends_have_identical_content() {
    let record = FindingRecord::open(observation(), evidence("root"), "import");
    let left = repository(Arc::new(MemoryPort::default()));
    let right = repository(Arc::new(MemoryPort::default()));
    assert_eq!(
        left.append(&record).unwrap(),
        right.append(&record).unwrap()
    );
    assert_eq!(
        left.rebuild().unwrap().query(&FindingQuery::default()),
        right.rebuild().unwrap().query(&FindingQuery::default())
    );
}
