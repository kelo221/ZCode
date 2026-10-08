use std::fs::{File, OpenOptions};
#[cfg(debug_assertions)]
use std::io::Read;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

const ROOT_PREFIX: &str = "zcode-gpui-settings-";
const MARKER_NAME: &str = ".zcode-gpui-isolated.json";
const LOCK_NAME: &str = ".zcode-gpui-isolated.lock";
#[cfg(debug_assertions)]
const TICKET_NAME: &str = ".zcode-gpui-acceptance.json";

#[derive(Debug)]
pub(crate) struct IsolatedSettings {
    root: PathBuf,
    _lock: File,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RootMarker {
    version: u8,
    identity: String,
    root: PathBuf,
}

#[cfg(debug_assertions)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HarnessTicket {
    version: u8,
    identity: String,
    token: String,
}

static ISOLATED: OnceLock<Arc<IsolatedSettings>> = OnceLock::new();

fn refused() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Invalid disposable acceptance capability",
    )
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec(value)?)
}

impl IsolatedSettings {
    pub(crate) fn create() -> io::Result<Arc<Self>> {
        let identity = uuid::Uuid::now_v7().to_string();
        let root = std::env::temp_dir().join(format!("{ROOT_PREFIX}{identity}"));
        std::fs::create_dir(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        }
        let root = root.canonicalize()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(root.join(LOCK_NAME))?;
        lock.try_lock().map_err(|_| refused())?;
        let owned = Arc::new(Self { root, _lock: lock });
        for path in [
            owned.home(),
            owned.data_base(),
            owned.workspace(),
            owned.temp(),
        ] {
            std::fs::create_dir(&path)?;
        }
        std::fs::create_dir_all(owned.home().join("AppData/Local"))?;
        std::fs::create_dir_all(owned.home().join("AppData/Roaming"))?;
        write_json(
            &owned.root.join(MARKER_NAME),
            &RootMarker {
                version: 1,
                identity: identity.clone(),
                root: owned.root.clone(),
            },
        )?;
        #[cfg(debug_assertions)]
        write_json(
            &owned.ticket_path(),
            &HarnessTicket {
                version: 1,
                identity,
                token: format!(
                    "{}{}",
                    uuid::Uuid::now_v7().simple(),
                    uuid::Uuid::now_v7().simple()
                ),
            },
        )?;
        Ok(owned)
    }

    #[cfg(debug_assertions)]
    pub(crate) fn ticket_path(&self) -> PathBuf {
        self.root.join(TICKET_NAME)
    }

    #[cfg(debug_assertions)]
    pub(crate) fn reattach(ticket: &Path, token: &str) -> io::Result<Arc<Self>> {
        if !ticket.is_absolute()
            || ticket.file_name().is_none_or(|n| n != TICKET_NAME)
            || ticket
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(refused());
        }
        let requested = ticket.parent().ok_or_else(refused)?;
        let root = requested.canonicalize()?;
        let temp = std::env::temp_dir().canonicalize()?;
        if linked(requested)? || root.parent() != Some(temp.as_path()) {
            return Err(refused());
        }
        let name = root
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(refused)?;
        let identity = name.strip_prefix(ROOT_PREFIX).ok_or_else(refused)?;
        if uuid::Uuid::parse_str(identity).is_err() {
            return Err(refused());
        }
        validate_tree(&root)?;
        for relative in [
            "home",
            "data",
            "workspace",
            "temp",
            "home/AppData/Local",
            "home/AppData/Roaming",
        ] {
            if !root.join(relative).is_dir() {
                return Err(refused());
            }
        }
        let marker: RootMarker = read_json(&root.join(MARKER_NAME))?;
        let capability: HarnessTicket = read_json(&root.join(TICKET_NAME))?;
        if marker.version != 1
            || capability.version != 1
            || marker.identity != identity
            || marker.root != root
            || capability.identity != identity
            || token.len() != 64
            || !token.bytes().all(|b| b.is_ascii_hexdigit())
            || capability.token != token
        {
            return Err(refused());
        }
        let lock_path = root.join(LOCK_NAME);
        if !std::fs::symlink_metadata(&lock_path)?.is_file() {
            return Err(refused());
        }
        let lock = OpenOptions::new().read(true).write(true).open(lock_path)?;
        // PID 名称不能阻止同一 scratch 的两个 Host 写入；OS 文件锁覆盖整个进程生命周期。
        lock.try_lock().map_err(|_| refused())?;
        validate_tree(&root)?;
        Ok(Arc::new(Self { root, _lock: lock }))
    }

    pub(crate) fn install(self: &Arc<Self>) {
        let _ = ISOLATED.set(self.clone());
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn home(&self) -> PathBuf {
        self.root.join("home")
    }
    pub(crate) fn data_base(&self) -> PathBuf {
        self.root.join("data")
    }
    pub(crate) fn workspace(&self) -> PathBuf {
        self.root.join("workspace")
    }
    pub(crate) fn temp(&self) -> PathBuf {
        self.root.join("temp")
    }

    pub(crate) fn contains_known_path(&self, path: &Path) -> bool {
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return false;
        }
        let root = self.root().to_string_lossy();
        let text = path.to_string_lossy();
        let lexical = Path::new(text.strip_prefix(r"\\?\").unwrap_or(&text))
            .starts_with(Path::new(root.strip_prefix(r"\\?\").unwrap_or(&root)));
        lexical && path.canonicalize().is_ok_and(|p| p.starts_with(&self.root))
    }

    pub(crate) fn allows_workspace(&self, path: &Path) -> bool {
        path.canonicalize()
            .is_ok_and(|canonical| canonical.starts_with(&self.root))
    }

    pub(crate) fn apply_child_environment(&self, env: &mut Vec<(String, String)>) {
        env.retain(|(key, _)| {
            matches!(
                key.to_ascii_uppercase().as_str(),
                "PATH"
                    | "SYSTEMROOT"
                    | "SYSTEMDRIVE"
                    | "COMSPEC"
                    | "PATHEXT"
                    | "WINDIR"
                    | "LANG"
                    | "LC_ALL"
                    | "ELECTRON_RUN_AS_NODE"
            )
        });
        for (key, path) in [
            ("HOME", self.home()),
            ("USERPROFILE", self.home()),
            ("ZCODE_DESKTOP_HOME_DIR", self.home()),
            ("ZCODE_DATA_BASE_DIR", self.data_base()),
            ("APPDATA", self.home().join("AppData/Roaming")),
            ("LOCALAPPDATA", self.home().join("AppData/Local")),
            ("TEMP", self.temp()),
            ("TMP", self.temp()),
            ("TMPDIR", self.temp()),
        ] {
            env.push((key.into(), path.to_string_lossy().into_owned()));
        }
        env.push(("ZCODE_RUNTIME_ENV".into(), "desktop".into()));
        env.push((
            "ZCODE_SERVICE_AUTHORITY_MODE".into(),
            "desktop-attached-remote".into(),
        ));
    }
}

#[cfg(debug_assertions)]
fn linked(path: &Path) -> io::Result<bool> {
    let metadata = std::fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Ok(true);
        }
    }
    Ok(metadata.file_type().is_symlink())
}

#[cfg(debug_assertions)]
fn validate_tree(root: &Path) -> io::Result<()> {
    let mut pending = vec![(root.to_owned(), 0)];
    let mut count = 0;
    while let Some((path, depth)) = pending.pop() {
        count += 1;
        if count > 20_000 || depth > 64 || linked(&path)? {
            return Err(refused());
        }
        if !path.canonicalize()?.starts_with(root) {
            return Err(refused());
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            for entry in std::fs::read_dir(&path)? {
                pending.push((entry?.path(), depth + 1));
                if pending.len() > 20_000 {
                    return Err(refused());
                }
            }
        } else if !metadata.is_file() {
            return Err(refused());
        }
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<T> {
    let mut bytes = Vec::new();
    File::open(path)?.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(refused());
    }
    serde_json::from_slice(&bytes).map_err(|_| refused())
}

pub(crate) fn active() -> Option<&'static Arc<IsolatedSettings>> {
    ISOLATED.get()
}

#[cfg(test)]
#[path = "isolation_tests.rs"]
mod tests;
