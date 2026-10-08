//! A matched protected-handoff fault, restricted to one original test domain.
use super::*;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;

enum Phase {
    Waiting,
    Hidden(Value),
    Restored,
}

pub(super) struct FailedStop {
    authority: PathBuf,
    authority_identity: Value,
    resource: PathBuf,
    command_identity: Value,
    acknowledgement_identity: Value,
    phase: Phase,
}

impl FailedStop {
    pub(super) fn new(fixture: &Fixture, resource: &Path) -> Self {
        for name in ["stop-fault-command", "stop-fault-acknowledgement"] {
            let path = resource.join(name);
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o666)
                .open(&path)
                .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        }
        Self {
            authority: fixture.directory.clone(),
            authority_identity: identity(&fs::symlink_metadata(&fixture.directory).unwrap()),
            resource: resource.to_owned(),
            command_identity: identity(
                &fs::symlink_metadata(resource.join("stop-fault-command")).unwrap(),
            ),
            acknowledgement_identity: identity(
                &fs::symlink_metadata(resource.join("stop-fault-acknowledgement")).unwrap(),
            ),
            phase: Phase::Waiting,
        }
    }

    pub(super) fn poll(&mut self, fixture: &Fixture) {
        assert_eq!(
            identity(&fs::symlink_metadata(&self.authority).unwrap()),
            self.authority_identity
        );
        let mut command = fs::OpenOptions::new()
            .read(true)
            .custom_flags(0o400000 | 0o4000)
            .open(self.resource.join("stop-fault-command"))
            .unwrap();
        let metadata = command.metadata().unwrap();
        assert!(metadata.is_file() && metadata.uid() == 0 && metadata.len() <= 16);
        assert_eq!(identity(&metadata), self.command_identity);
        let mut bytes = Vec::new();
        Read::by_ref(&mut command)
            .take(17)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.len() <= 16);
        let original = self.authority.join("handoff-1.json");
        let saved = self.authority.join("fixture-handoff-1.saved");
        match (&self.phase, bytes.as_slice()) {
            (Phase::Waiting, b"hide") => {
                assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 1);
                let binding = read_value(&self.authority.join("binding-1.json"), true).unwrap();
                let claim =
                    fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap())
                        .unwrap();
                let snapshot = Store::open_read_only(&fixture.store).unwrap();
                assert_eq!(
                    binding.get("journal_claim"),
                    Some(&Value::Str(
                        snapshot.current_execution_claim_hash(&claim).unwrap()
                    ))
                );
                assert_eq!(
                    claim.domain().to_value().get("authority"),
                    Some(&self.authority_identity)
                );
                let handoff = read_value(&original, true).unwrap();
                assert_eq!(handoff.get("binding"), Some(&binding));
                assert!(!self.authority.join("completed-1-1.json").exists());
                assert!(!self.authority.join("closure-1-1.json").exists());
                assert!(!saved.try_exists().unwrap());
                let expected = identity(&fs::symlink_metadata(&original).unwrap());
                fs::rename(&original, &saved).unwrap();
                fs::File::open(&self.authority).unwrap().sync_all().unwrap();
                self.phase = Phase::Hidden(expected);
                self.acknowledge(b"hidden");
            }
            (Phase::Hidden(expected), b"restore") => {
                assert_eq!(identity(&fs::symlink_metadata(&saved).unwrap()), *expected);
                assert!(!original.try_exists().unwrap());
                fs::rename(&saved, &original).unwrap();
                fs::File::open(&self.authority).unwrap().sync_all().unwrap();
                self.phase = Phase::Restored;
                self.acknowledge(b"restored");
            }
            _ => {}
        }
    }

    fn acknowledge(&self, bytes: &[u8]) {
        let mut acknowledgement = fs::OpenOptions::new()
            .write(true)
            .custom_flags(0o400000 | 0o4000)
            .open(self.resource.join("stop-fault-acknowledgement"))
            .unwrap();
        let metadata = acknowledgement.metadata().unwrap();
        assert!(metadata.is_file() && metadata.uid() == 0);
        assert_eq!(identity(&metadata), self.acknowledgement_identity);
        acknowledgement.set_len(0).unwrap();
        std::io::Write::write_all(&mut acknowledgement, bytes).unwrap();
        acknowledgement.sync_all().unwrap();
    }

    pub(super) fn restored(&self) -> bool {
        matches!(self.phase, Phase::Restored)
    }
}
