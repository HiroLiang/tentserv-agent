use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use crate::features::model::{
    compatibility::CompatibilityFilter,
    domain::{
        ModelCapability, ModelCapabilityProof, ModelCapabilityProofKey, ModelCapabilityProofStatus,
    },
    ports::{ModelCapabilityProofStore, ModelCompatibilityProofStore},
};
use crate::features::resource_coordination::{
    infra::FileResourceCoordinator, ResourceCoordinator, ResourceKey, ResourceKind,
    ResourceLockMode, ResourceLockRequest,
};

use super::{fixtures::Fixture, FileModelCapabilityProofStore};
use crate::foundation::error::{KernelError, KernelResult};

const WORKER: &str = "features::model::infra::proof::tests::processes::proof_process_worker";
const WAIT_LIMIT: Duration = Duration::from_secs(15);
// Each scenario still races three OS processes. Do not multiply fsync-heavy
// scenarios by the test harness's host-dependent worker count.
static PROCESS_SCENARIO: Mutex<()> = Mutex::new(());

struct ProofChild {
    child: Child,
    root: PathBuf,
    name: String,
}

impl ProofChild {
    fn spawn(fixture: &Fixture, name: &str, mode: &str, backend: &str) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", WORKER, "--nocapture"])
            .env("TENTGENT_PROOF_TEST_ROOT", &fixture.root)
            .env("TENTGENT_PROOF_TEST_NAME", name)
            .env("TENTGENT_PROOF_TEST_MODE", mode)
            .env("TENTGENT_PROOF_TEST_BACKEND", backend)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            child,
            root: fixture.root.clone(),
            name: name.into(),
        }
    }

    fn ready(&mut self) {
        let started = Instant::now();
        while !self.root.join(format!("{}.ready", self.name)).exists() {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "worker {} exited before ready",
                self.name
            );
            assert!(
                started.elapsed() < WAIT_LIMIT,
                "worker {} never became ready",
                self.name
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn finish(&mut self) -> ExitStatus {
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                started.elapsed() < WAIT_LIMIT,
                "worker {} did not finish",
                self.name
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn successful(&mut self) {
        let status = self.finish();
        let mut stderr = String::new();
        self.child
            .stderr
            .as_mut()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(status.success(), "worker {} failed: {stderr}", self.name);
    }
}

impl Drop for ProofChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn concurrent_legacy_operations(second_backend: &str, second_mode: &str) -> usize {
    let _scenario = PROCESS_SCENARIO.lock().unwrap();
    let fixture = Fixture::new("concurrent");
    FileModelCapabilityProofStore
        .save_capability_proof(&fixture.context(), &fixture.proof("same"))
        .unwrap();
    let mut writer_a = ProofChild::spawn(&fixture, "a", "write", "same");
    let mut writer_b = ProofChild::spawn(&fixture, "b", second_mode, second_backend);
    let mut reader = ProofChild::spawn(&fixture, "reader", "read", "same");
    writer_a.ready();
    writer_b.ready();
    reader.ready();
    fs::write(fixture.root.join("go"), b"go").unwrap();
    writer_a.successful();
    writer_b.successful();
    reader.successful();
    FileModelCapabilityProofStore
        .list_capability_proofs_for(
            &fixture.context(),
            fixture.model_ref(),
            ModelCapability::Chat,
        )
        .unwrap()
        .len()
}

#[test]
fn independent_processes_replace_same_legacy_tuple_without_torn_snapshot() {
    assert_eq!(concurrent_legacy_operations("same", "write"), 1);
}

#[test]
fn independent_processes_preserve_distinct_legacy_tuples() {
    assert_eq!(concurrent_legacy_operations("other", "write"), 2);
}

#[test]
fn independent_process_clear_and_write_remain_serialized() {
    assert!(concurrent_legacy_operations("same", "clear") <= 1);
}

fn concurrent_exact_operations(second_version: &str, second_mode: &str) -> usize {
    let _scenario = PROCESS_SCENARIO.lock().unwrap();
    let fixture = Fixture::new("concurrent-v2");
    FileModelCapabilityProofStore
        .save_exact(&fixture.context(), &fixture.proof_v2("1.0", 0))
        .unwrap();
    let mut writer_a = ProofChild::spawn(&fixture, "a", "v2-write", "1.0");
    let mut writer_b = ProofChild::spawn(&fixture, "b", second_mode, second_version);
    let mut reader = ProofChild::spawn(&fixture, "reader", "v2-read", "1.0");
    writer_a.ready();
    writer_b.ready();
    reader.ready();
    fs::write(fixture.root.join("go"), b"go").unwrap();
    writer_a.successful();
    writer_b.successful();
    reader.successful();
    FileModelCapabilityProofStore
        .list_exact(
            &fixture.context(),
            fixture.model_ref(),
            &CompatibilityFilter::default(),
        )
        .unwrap()
        .len()
}

#[test]
fn independent_processes_replace_same_exact_v2_tuple() {
    assert_eq!(concurrent_exact_operations("1.0", "v2-write"), 1);
}

#[test]
fn independent_processes_preserve_distinct_exact_v2_tuples() {
    assert_eq!(concurrent_exact_operations("2.0", "v2-write"), 2);
}

#[test]
fn independent_process_bulk_clear_and_v2_write_remain_serialized() {
    assert!(concurrent_exact_operations("1.0", "clear") <= 1);
}

#[test]
fn abrupt_process_exit_releases_locks_and_ignores_uncommitted_temporary_file() {
    let _scenario = PROCESS_SCENARIO.lock().unwrap();
    let fixture = Fixture::new("abrupt-exit");
    let proof = fixture.proof("same");
    FileModelCapabilityProofStore
        .save_capability_proof(&fixture.context(), &proof)
        .unwrap();
    let mut child = ProofChild::spawn(&fixture, "crash", "crash", "same");
    child.ready();
    let busy = FileResourceCoordinator
        .acquire(
            &fixture.runtime,
            ResourceLockRequest::new(
                "delete-while-writing",
                vec![(
                    ResourceKey::new(ResourceKind::Model, fixture.model_ref().as_str()),
                    ResourceLockMode::Exclusive,
                )],
            )
            .with_limits(Duration::from_millis(20), 1),
        )
        .unwrap();
    assert!(
        busy.is_err(),
        "proof writer must also protect the model directory"
    );
    let error = FileModelCapabilityProofStore
        .list_capability_proofs_for(
            &fixture.context(),
            fixture.model_ref(),
            ModelCapability::Chat,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        crate::foundation::error::KernelError::ResourceCoordinationUnavailable(_)
    ));
    fs::write(fixture.root.join("go"), b"exit without unwinding").unwrap();
    assert_eq!(child.finish().code(), Some(23));
    assert_eq!(
        FileModelCapabilityProofStore
            .list_capability_proofs_for(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat
            )
            .unwrap(),
        vec![proof.clone()]
    );
    let directory = fixture
        .store
        .support_proofs_capability_dir(fixture.model_ref(), ModelCapability::Chat);
    assert!(directory.join(".uncommitted.toml.crash.tmp").exists());
    FileModelCapabilityProofStore
        .save_capability_proof(&fixture.context(), &proof)
        .unwrap();
}

#[test]
fn process_exit_after_primary_replace_preserves_authority_and_retry_repairs_mirror() {
    let _scenario = PROCESS_SCENARIO.lock().unwrap();
    let fixture = Fixture::new("exit-after-primary");
    let store = FileModelCapabilityProofStore;
    let original = fixture.proof("same");
    store
        .save_capability_proof(&fixture.context(), &original)
        .unwrap();
    let latest = fixture
        .store
        .capability_proof_path(fixture.model_ref(), ModelCapability::Chat);
    let original_mirror = fs::read(&latest).unwrap();
    let replacement = checkpoint_replacement(&fixture, "same");
    let primary = fixture
        .store
        .support_proof_path(&ModelCapabilityProofKey::from_proof(&replacement));

    let mut child = ProofChild::spawn(&fixture, "primary", "after-primary", "same");
    child.ready();
    assert_eq!(fs::read(&latest).unwrap(), original_mirror);
    assert_eq!(
        toml::from_str::<ModelCapabilityProof>(&fs::read_to_string(&primary).unwrap()).unwrap(),
        replacement
    );
    fs::write(fixture.root.join("go"), b"exit after primary replacement").unwrap();
    assert_eq!(child.finish().code(), Some(23));

    // The old verified mirror must not override the newer failed primary with
    // the same key, even though process exit bypassed the permit's destructor.
    assert_eq!(
        store
            .list_capability_proofs_for(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat,
            )
            .unwrap(),
        vec![replacement.clone()]
    );
    store
        .save_capability_proof(&fixture.context(), &replacement)
        .unwrap();
    assert_eq!(fs::read(&primary).unwrap(), fs::read(&latest).unwrap());
    assert_eq!(
        toml::from_str::<ModelCapabilityProof>(&fs::read_to_string(&latest).unwrap()).unwrap(),
        replacement
    );
    assert_eq!(
        store
            .list_capability_proofs_for(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat,
            )
            .unwrap(),
        vec![replacement]
    );
}

#[test]
fn process_exit_during_bulk_clear_allows_counted_retry_without_clearing_other_capabilities() {
    let _scenario = PROCESS_SCENARIO.lock().unwrap();
    let fixture = Fixture::new("exit-during-clear");
    let store = FileModelCapabilityProofStore;
    let chat = fixture.proof("same");
    let exact_chat = fixture.proof_v2("1.0", 0);
    store
        .save_capability_proof(&fixture.context(), &chat)
        .unwrap();
    let chat_key = store.save_exact(&fixture.context(), &exact_chat).unwrap();
    let mut embedding = fixture.proof("same");
    embedding.capability = ModelCapability::Embedding;
    let exact_embedding = fixture.proof_v2_for(ModelCapability::Embedding, "1.0", 0);
    store
        .save_capability_proof(&fixture.context(), &embedding)
        .unwrap();
    let embedding_key = store
        .save_exact(&fixture.context(), &exact_embedding)
        .unwrap();
    let other_paths = [
        fixture
            .store
            .capability_proof_path(fixture.model_ref(), ModelCapability::Embedding),
        fixture
            .store
            .support_proof_path(&ModelCapabilityProofKey::from_proof(&embedding)),
        fixture.store.compatibility_proof_path(&embedding_key),
    ];
    let other_bytes = other_paths.each_ref().map(|path| fs::read(path).unwrap());

    let mut child = ProofChild::spawn(&fixture, "clear", "during-clear", "same");
    child.ready();
    assert!(!fixture
        .store
        .capability_proof_path(fixture.model_ref(), ModelCapability::Chat)
        .exists());
    assert!(fixture
        .store
        .support_proof_path(&ModelCapabilityProofKey::from_proof(&chat))
        .is_file());
    assert!(fixture.store.compatibility_proof_path(&chat_key).is_file());
    fs::write(fixture.root.join("go"), b"exit during capability clear").unwrap();
    assert_eq!(child.finish().code(), Some(23));

    assert_eq!(
        store
            .list_capability_proofs_for(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat,
            )
            .unwrap(),
        vec![chat]
    );
    assert_eq!(
        store.get_exact(&fixture.context(), &chat_key).unwrap(),
        Some(exact_chat)
    );
    for expected_count in [2, 0] {
        assert_eq!(
            store
                .remove_capability_proof(
                    &fixture.context(),
                    fixture.model_ref(),
                    ModelCapability::Chat,
                )
                .unwrap(),
            expected_count
        );
    }
    assert!(store
        .list_capability_proofs_for(
            &fixture.context(),
            fixture.model_ref(),
            ModelCapability::Chat,
        )
        .unwrap()
        .is_empty());
    assert!(store
        .get_exact(&fixture.context(), &chat_key)
        .unwrap()
        .is_none());
    assert_eq!(
        store
            .list_capability_proofs_for(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Embedding,
            )
            .unwrap(),
        vec![embedding]
    );
    assert_eq!(
        store.get_exact(&fixture.context(), &embedding_key).unwrap(),
        Some(exact_embedding)
    );
    assert_eq!(
        other_paths.each_ref().map(|path| fs::read(path).unwrap()),
        other_bytes
    );
}

#[test]
fn waiting_writer_does_not_recreate_a_model_deleted_under_exclusive_model_lock() {
    let _scenario = PROCESS_SCENARIO.lock().unwrap();
    let fixture = Fixture::new("deleted-model");
    let permit = FileResourceCoordinator
        .acquire(
            &fixture.runtime,
            ResourceLockRequest::new(
                "delete-model",
                vec![
                    (ResourceKey::maintenance(), ResourceLockMode::Shared),
                    (
                        ResourceKey::new(ResourceKind::Model, fixture.model_ref().as_str()),
                        ResourceLockMode::Exclusive,
                    ),
                ],
            ),
        )
        .unwrap()
        .unwrap();
    let mut child = ProofChild::spawn(&fixture, "deleted", "deleted", "same");
    child.ready();
    fs::remove_dir_all(fixture.store.model_dir(fixture.model_ref())).unwrap();
    fs::write(fixture.root.join("go"), b"attempt write").unwrap();
    drop(permit);
    child.successful();
    assert!(!fixture.store.model_dir(fixture.model_ref()).exists());
}

fn wait_for_go(fixture: &Fixture) {
    let started = Instant::now();
    while !fixture.root.join("go").exists() {
        assert!(
            started.elapsed() < WAIT_LIMIT,
            "parent never released the barrier"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn checkpoint_replacement(fixture: &Fixture, backend: &str) -> ModelCapabilityProof {
    let mut proof = fixture.proof(backend);
    proof.status = ModelCapabilityProofStatus::Failed;
    proof.checked_at = "2026-10-08T00:00:59Z".into();
    proof.error = Some("checkpoint load failure".into());
    proof
}

fn assert_consistent_legacy_snapshot(fixture: &Fixture) {
    let permit = retry_busy(|| {
        FileResourceCoordinator
            .acquire(
                &fixture.runtime,
                ResourceLockRequest::new(
                    "snapshot-reader",
                    fixture.locks(ResourceLockMode::Shared),
                ),
            )?
            .map_err(|busy| KernelError::ResourceCoordinationUnavailable(busy.description))
    })
    .unwrap();
    let context = fixture.context().with_permit(&permit);
    let records = FileModelCapabilityProofStore
        .list_capability_proofs_for(&context, fixture.model_ref(), ModelCapability::Chat)
        .unwrap();
    let latest = fixture
        .store
        .capability_proof_path(fixture.model_ref(), ModelCapability::Chat);
    if latest.exists() {
        let mirror: ModelCapabilityProof =
            toml::from_str(&fs::read_to_string(latest).unwrap()).unwrap();
        let primary = fixture
            .store
            .support_proof_path(&ModelCapabilityProofKey::from_proof(&mirror));
        let primary: ModelCapabilityProof =
            toml::from_str(&fs::read_to_string(primary).unwrap()).unwrap();
        assert_eq!(
            primary, mirror,
            "reader must not observe the gap between primary and mirror commits"
        );
        assert!(records.contains(&mirror));
    } else {
        assert!(
            records.is_empty(),
            "reader must not observe a half-cleared legacy pair"
        );
    }
}

fn retry_busy<T>(mut operation: impl FnMut() -> KernelResult<T>) -> KernelResult<T> {
    for attempt in 0..4 {
        match operation() {
            Err(KernelError::ResourceCoordinationUnavailable(message))
                if message.contains(" is busy;") && attempt < 3 =>
            {
                // Busy is an expected bounded outcome, not a persistence failure.
                thread::sleep(Duration::from_millis(25));
            }
            result => return result,
        }
    }
    unreachable!("the final attempt returns its result")
}

#[test]
#[ignore = "subprocess helper"]
fn proof_process_worker() {
    let Some(root) = std::env::var_os("TENTGENT_PROOF_TEST_ROOT") else {
        return;
    };
    let fixture = Fixture::at(&PathBuf::from(root));
    let name = std::env::var("TENTGENT_PROOF_TEST_NAME").unwrap();
    let mode = std::env::var("TENTGENT_PROOF_TEST_MODE").unwrap();
    let backend = std::env::var("TENTGENT_PROOF_TEST_BACKEND").unwrap();
    let store = FileModelCapabilityProofStore;
    if matches!(mode.as_str(), "crash" | "after-primary" | "during-clear") {
        let permit = fixture.permit(ResourceLockMode::Exclusive);
        // Deterministic crash checkpoints, not a kill injected inside a system
        // call. All mutations happen under the same real exclusive permit.
        match mode.as_str() {
            "crash" => {
                let directory = fixture
                    .store
                    .support_proofs_capability_dir(fixture.model_ref(), ModelCapability::Chat);
                fs::write(
                    directory.join(".uncommitted.toml.crash.tmp"),
                    b"incomplete = [",
                )
                .unwrap();
            }
            "after-primary" => store
                .save_support_proof(
                    &fixture.context().with_permit(&permit),
                    &checkpoint_replacement(&fixture, &backend),
                )
                .unwrap(),
            "during-clear" => fs::remove_file(
                fixture
                    .store
                    .capability_proof_path(fixture.model_ref(), ModelCapability::Chat),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        fs::write(fixture.root.join(format!("{name}.ready")), b"ready").unwrap();
        wait_for_go(&fixture);
        std::process::exit(23); // Bypass Drop to exercise OS-level lock release.
    }
    fs::write(fixture.root.join(format!("{name}.ready")), b"ready").unwrap();
    wait_for_go(&fixture);
    match mode.as_str() {
        "v2-write" => {
            for iteration in 0..20 {
                retry_busy(|| {
                    store.save_exact(&fixture.context(), &fixture.proof_v2(&backend, iteration))
                })
                .unwrap();
            }
        }
        "write" => {
            for iteration in 0..20 {
                let mut proof = fixture.proof(&backend);
                proof.checked_at = format!("2026-10-08T00:00:{iteration:02}Z");
                retry_busy(|| store.save_capability_proof(&fixture.context(), &proof)).unwrap();
            }
        }
        "clear" => {
            for _ in 0..20 {
                retry_busy(|| {
                    store.remove_capability_proof(
                        &fixture.context(),
                        fixture.model_ref(),
                        ModelCapability::Chat,
                    )
                })
                .unwrap();
            }
        }
        "read" | "v2-read" => {
            let started = Instant::now();
            loop {
                if mode == "read" {
                    assert_consistent_legacy_snapshot(&fixture);
                } else {
                    retry_busy(|| {
                        store.list_exact(
                            &fixture.context(),
                            fixture.model_ref(),
                            &CompatibilityFilter {
                                capability: Some(ModelCapability::Chat),
                                ..Default::default()
                            },
                        )
                    })
                    .expect("v2 snapshot must not contain torn or mismatched records");
                }
                if fixture.root.join("a.done").exists() && fixture.root.join("b.done").exists() {
                    break;
                }
                assert!(started.elapsed() < WAIT_LIMIT, "writers did not finish");
                thread::sleep(Duration::from_millis(1));
            }
        }
        "deleted" => {
            assert!(store
                .save_capability_proof(&fixture.context(), &fixture.proof(&backend))
                .is_err());
            assert!(!fixture.store.model_dir(fixture.model_ref()).exists());
        }
        _ => panic!("unknown worker mode"),
    }
    fs::write(fixture.root.join(format!("{name}.done")), b"done").unwrap();
}
