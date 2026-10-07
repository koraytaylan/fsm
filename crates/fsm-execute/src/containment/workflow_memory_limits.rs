//! Namespace-specific test service limits, retained with any failed fixture.
use super::*;
use std::os::unix::fs::OpenOptionsExt;

const CONFIGURATION: &[u8] = b"[Service]\nMemoryMax=1G\nMemorySwapMax=0\n";

pub(super) struct Limits {
    directory: PathBuf,
    directory_identity: Value,
    file_identity: Value,
}

impl Limits {
    pub(super) fn install(fixture: &Fixture) -> Self {
        let namespace = fixture.directory.parent().unwrap().file_name().unwrap();
        let directory = Path::new("/run/systemd/system").join(format!(
            "fsm-containment-{}-.service.d",
            namespace.to_str().unwrap()
        ));
        super::super::super::protected_directory(directory.parent().unwrap()).unwrap();
        fs::DirBuilder::new()
            .mode(0o755)
            .create(&directory)
            .unwrap();
        let path = directory.join("90-fsm-workflow-memory.conf");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&path)
            .unwrap();
        file.write_all(CONFIGURATION).unwrap();
        file.sync_all().unwrap();
        let limits = Self {
            directory_identity: identity(&fs::symlink_metadata(&directory).unwrap()),
            file_identity: identity(&file.metadata().unwrap()),
            directory,
        };
        assert!(
            Command::new("/usr/bin/systemctl")
                .arg("daemon-reload")
                .status()
                .unwrap()
                .success()
        );
        limits
    }

    pub(super) fn inventory(&self) -> Value {
        object([
            (
                "directory",
                Value::Str(self.directory.to_str().unwrap().into()),
            ),
            ("directory_identity", self.directory_identity.clone()),
            ("file_identity", self.file_identity.clone()),
        ])
    }

    pub(super) fn retire(self) {
        let metadata = fs::symlink_metadata(&self.directory).unwrap();
        assert!(metadata.is_dir() && metadata.uid() == 0);
        assert_eq!(identity(&metadata), self.directory_identity);
        let path = self.directory.join("90-fsm-workflow-memory.conf");
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file() && metadata.uid() == 0);
        assert_eq!(identity(&metadata), self.file_identity);
        assert_eq!(fs::read(&path).unwrap(), CONFIGURATION);
        assert_eq!(fs::read_dir(&self.directory).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(self.directory).unwrap();
        assert!(
            Command::new("/usr/bin/systemctl")
                .arg("daemon-reload")
                .status()
                .unwrap()
                .success()
        );
    }
}
