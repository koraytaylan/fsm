use super::{finite_timeout, invalid, owner_uid, private_directory, protocol, server};
use fsm_core::json::write_canonical;
use fsm_execute::service::OwnedNativeExecutor;
use std::{
    fs::{self, DirBuilder, File, Metadata, OpenOptions, Permissions},
    io::{self, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// An explicitly published endpoint, separate from native shutdown ownership.
pub struct LocalControlEndpoint {
    directory: PathBuf,
    stop: Arc<AtomicBool>,
    cleaned: Arc<AtomicBool>,
}

impl LocalControlEndpoint {
    /// Publish beneath an existing private root for the driver's original writer.
    /// The root must have mode 0700 and belong to this effective process owner.
    pub fn publish(root: &Path, driver: &mut OwnedNativeExecutor) -> io::Result<Self> {
        private_directory(root, owner_uid()?)?;
        let store = driver
            .store_mut()
            .ok_or_else(|| invalid("writer already released"))?;
        let physical = fs::metadata(&store.data_dir)?;
        let mut random = [0u8; 32];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let incarnation: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let identity = protocol::ControlIdentity {
            incarnation: incarnation.clone(),
            store_device: physical.dev(),
            store_inode: physical.ino(),
        };
        let directory = root.join(format!("c-{}", &incarnation[..16]));
        let socket = directory.join("s");
        if socket.as_os_str().as_encoded_bytes().len() > 107 {
            return Err(invalid(
                "control socket pathname exceeds Unix transport limit",
            ));
        }
        DirBuilder::new().mode(0o700).create(&directory)?;
        let mut cleanup = Cleanup {
            directory: directory.clone(),
            original: fs::symlink_metadata(&directory)?,
            files: Vec::new(),
            armed: true,
        };
        let listener = UnixListener::bind(&socket)?;
        cleanup.capture(&socket)?;
        fs::set_permissions(&socket, Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let metadata = directory.join("identity");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&metadata)?;
        cleanup.capture(&metadata)?;
        let mut bytes = Vec::new();
        write_canonical(&protocol::identity_value(&identity), &mut bytes);
        file.write_all(&bytes)?;
        drop(file);
        let stop = Arc::new(AtomicBool::new(false));
        let cleaned = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_cleaned = cleaned.clone();
        let control = driver.control();
        std::thread::Builder::new()
            .name("fsm-local-control".into())
            .spawn(move || {
                server::run(listener, control, identity, &thread_stop);
                // Filesystem cleanup can stall; callers wait only to their own bound.
                thread_cleaned.store(cleanup.remove(), Ordering::Release);
            })?;
        Ok(Self {
            directory,
            stop,
            cleaned,
        })
    }

    /// Published directory for this exact incarnation, useful for diagnostics.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Stop transport admission and wait at most the validated finite timeout.
    /// True confirms this endpoint's files were removed; it says nothing about
    /// native shutdown, journal writer release or outstanding response delivery.
    pub fn close(&self, timeout_ms: i64) -> io::Result<bool> {
        let deadline = Instant::now() + finite_timeout(timeout_ms)?;
        self.stop.store(true, Ordering::Release);
        while !self.cleaned.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(self.cleaned.load(Ordering::Acquire))
    }
}

impl Drop for LocalControlEndpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

struct Cleanup {
    directory: PathBuf,
    original: Metadata,
    files: Vec<(PathBuf, Metadata)>,
    armed: bool,
}
impl Cleanup {
    fn capture(&mut self, path: &Path) -> io::Result<()> {
        self.files.push((path.into(), fs::symlink_metadata(path)?));
        Ok(())
    }
    fn remove(&mut self) -> bool {
        let same = |path: &Path, original: &Metadata| {
            fs::symlink_metadata(path)
                .is_ok_and(|now| now.dev() == original.dev() && now.ino() == original.ino())
        };
        if !same(&self.directory, &self.original) {
            return false;
        }
        for (path, original) in &self.files {
            if !same(path, original) || fs::remove_file(path).is_err() {
                return false;
            }
        }
        self.files.clear();
        let removed = fs::remove_dir(&self.directory).is_ok();
        if removed {
            self.armed = false;
        }
        removed
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        // Publication failures also clean only identities this publisher captured.
        if self.armed {
            let _ = self.remove();
        }
    }
}
