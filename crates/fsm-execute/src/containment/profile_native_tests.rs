//! Early refusal mutates only the exclusive provisioner's original fixture inode.

use super::{Fixture, prepare};
use fsm_core::sha256::{sha256, to_hex};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

struct Restore(File);

impl Drop for Restore {
    fn drop(&mut self) {
        // Restore through the original descriptor, never a replacement path.
        let _ = self
            .0
            .set_permissions(std::fs::Permissions::from_mode(0o711));
    }
}

pub(super) fn run() {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(super::super::super::NOFOLLOW_NONBLOCK)
        .open(super::super::super::enrollment::EXECUTABLE)
        .unwrap();
    let metadata = file.metadata().unwrap();
    let expected = |name: &str| std::env::var(name).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.uid(), 0);
    assert_eq!(metadata.mode() & 0o7777, 0o711);
    assert_eq!(
        metadata.dev().to_string(),
        expected("FSM_NATIVE_FIXTURE_DEVICE")
    );
    assert_eq!(
        metadata.ino().to_string(),
        expected("FSM_NATIVE_FIXTURE_INODE")
    );
    let mut bytes = Vec::new();
    (&mut file)
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 64 * 1024 * 1024);
    assert_eq!(
        to_hex(&sha256(&bytes)),
        expected("FSM_NATIVE_FIXTURE_SHA256")
    );
    drop(bytes);
    let restore = Restore(file);
    let mut fixture = Fixture::new();
    let before = fixture.counter();
    let inventory = || {
        let mut names = std::fs::read_dir(&fixture.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let original = inventory();
    restore
        .0
        .set_permissions(std::fs::Permissions::from_mode(0o755))
        .unwrap();
    assert!(
        prepare(&fixture.directory)
            .unwrap_err()
            .contains("installed gate executable")
    );
    assert_eq!(fixture.counter(), before);
    assert_eq!(inventory(), original);
    assert!(fixture.groups.is_empty());
    restore
        .0
        .set_permissions(std::fs::Permissions::from_mode(0o711))
        .unwrap();
    let domain = fixture.prepare();
    assert_eq!(super::number(&domain, "allocation").unwrap(), 1);
    fixture.cleanup().unwrap();
}
