use super::models::AppConfig;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Abstraction over the file operations used by `atomic_write`, so tests
/// can inject failures (fault injection) without touching the real disk.
pub trait FileOps {
    fn write_file(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    fn sync_file(&self, path: &Path) -> io::Result<()>;
    fn rename_file(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

pub struct RealFileOps;

impl FileOps for RealFileOps {
    fn write_file(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        fs::write(path, data)
    }
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        // Best-effort durability: flush file data + metadata before the
        // rename. This is NOT a power-loss proof (see ADR "safe-replace"
        // in docs/ARCHITECTURE.md).
        fs::OpenOptions::new().write(true).open(path)?.sync_all()
    }
    fn rename_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unique temp file next to the target (same volume, so the rename below
/// stays atomic). Never shared between writers: pid + process-wide counter
/// + timestamp make collisions practically impossible.
fn unique_tmp_path(path: &Path) -> PathBuf {
    let ctr = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut tmp = path.to_path_buf();
    tmp.set_extension(format!("json.tmp.{pid}.{ctr}.{nanos}"));
    tmp
}

/// Atomically replace `path` with `data`.
///
/// ADR "safe-replace" (docs/ARCHITECTURE.md):
/// - The temp file is written next to the target (same volume) under a
///   unique name, so concurrent writers never share it and never see a
///   half-written file.
/// - The existing file is NEVER deleted first. `std::fs::rename` on
///   Windows maps to `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`; on
///   the same volume the destination is replaced atomically — readers see
///   either the old or the new file, never a torn or missing one. If the
///   rename fails, the original is untouched and an error is returned.
/// - `sync_all` before the rename is best-effort durability, not a
///   power-loss proof. Do not claim power-loss durability from unit tests.
fn atomic_write_with_ops(path: &Path, data: &[u8], ops: &dyn FileOps) -> io::Result<()> {
    let tmp = unique_tmp_path(path);
    let result = (|| {
        ops.write_file(&tmp, data)?;
        ops.sync_file(&tmp)?;
        ops.rename_file(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        // Best-effort cleanup of our own temp file. The original settings
        // file is untouched because we never delete-then-rename.
        let _ = ops.remove_file(&tmp);
    }
    result
}

fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    atomic_write_with_ops(path, data, &RealFileOps)
}

pub fn save(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
        }
    }
    let data = serde_json::to_vec_pretty(config).map_err(|e| format!("serialize settings: {e}"))?;
    atomic_write(path, &data).map_err(|e| format!("write settings: {e}"))
}

pub fn load(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
            Ok(cfg) => {
                tracing::info!("loaded settings from {}", path.display());
                cfg.migrate()
            }
            Err(err) => {
                tracing::error!("settings corrupted ({err}); restoring defaults");
                backup_corrupt(path);
                AppConfig::default()
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            tracing::info!("no settings file yet; using defaults");
            AppConfig::default()
        }
        Err(err) => {
            tracing::error!("failed to read settings ({err}); using defaults");
            AppConfig::default()
        }
    }
}

fn backup_corrupt(path: &Path) {
    let mut bak = path.to_path_buf();
    bak.set_extension("json.bak");
    if let Err(err) = fs::rename(path, &bak) {
        tracing::warn!("could not backup corrupt settings: {err}");
    } else {
        tracing::warn!("corrupt settings moved to {}", bak.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::models::Profile;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};

    fn temp_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("keyglow-{name}-{nanos}.json"))
    }

    fn sample_config() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.profiles.push(Profile {
            id: "custom".into(),
            name: "Custom".into(),
            layout: "60-ansi".into(),
            disabled_keys: vec!["CapsLock".into()],
            builtin: false,
        });
        // Activation invariant (HF-06): selected_layout follows the selected
        // profile's layout, so select the custom profile to keep 60-ansi.
        cfg.selected_profile = "custom".into();
        cfg.selected_layout = "60-ansi".into();
        cfg
    }

    #[test]
    fn roundtrip_and_corrupt_fallback() {
        let path = temp_file("ok");
        let cfg = sample_config();
        save(&path, &cfg).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.selected_layout, "60-ansi");
        assert!(loaded.profiles.iter().any(|p| p.id == "custom"));

        fs::write(&path, "{not json").unwrap();
        let recovered = load(&path);
        assert_eq!(recovered.selected_profile, "default");
        assert!(!recovered.profiles.is_empty());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn missing_file_returns_defaults() {
        let path = temp_file("missing");
        let _ = fs::remove_file(&path);
        let cfg = load(&path);
        assert_eq!(cfg.profiles.len(), 3);
        assert_eq!(cfg.selected_profile, "default");
    }

    /// Fault injection: every file-operation stage can fail, and the
    /// original file must survive each failure with an error returned.
    struct FailingOps {
        fail_at: &'static str,
    }

    impl FileOps for FailingOps {
        fn write_file(&self, path: &Path, data: &[u8]) -> io::Result<()> {
            if self.fail_at == "write" {
                return Err(io::Error::new(io::ErrorKind::Other, "injected write failure"));
            }
            RealFileOps.write_file(path, data)
        }
        fn sync_file(&self, path: &Path) -> io::Result<()> {
            if self.fail_at == "sync" {
                return Err(io::Error::new(io::ErrorKind::Other, "injected sync failure"));
            }
            RealFileOps.sync_file(path)
        }
        fn rename_file(&self, from: &Path, to: &Path) -> io::Result<()> {
            if self.fail_at == "rename" {
                return Err(io::Error::new(io::ErrorKind::Other, "injected rename failure"));
            }
            RealFileOps.rename_file(from, to)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            RealFileOps.remove_file(path)
        }
    }

    #[test]
    fn failed_replace_preserves_original() {
        for fail_at in ["write", "sync", "rename"] {
            let path = temp_file(&format!("fault-{fail_at}"));
            let original = b"{\"version\":1}".to_vec();
            fs::write(&path, &original).unwrap();

            let ops = FailingOps { fail_at };
            let err = atomic_write_with_ops(&path, b"{\"version\":2}", &ops)
                .expect_err("injected failure must surface");
            assert!(
                err.to_string().contains("injected"),
                "unexpected error: {err}"
            );
            // The original file is untouched: we never delete-then-rename.
            assert_eq!(fs::read(&path).unwrap(), original);
            // No temp litter left behind.
            let parent = path.parent().unwrap();
            let stem = path.file_name().unwrap().to_string_lossy();
            let litter: Vec<_> = fs::read_dir(parent)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(stem.as_ref()) && n.contains(".tmp."))
                .collect();
            assert!(litter.is_empty(), "temp litter: {litter:?}");
            let _ = fs::remove_file(&path);
        }
    }

    #[test]
    fn temp_names_are_unique_across_writers() {
        let path = temp_file("uniqueness");
        let mut names = HashSet::new();
        for _ in 0..500 {
            let tmp = unique_tmp_path(&path);
            assert!(names.insert(tmp.clone()), "duplicate temp name: {tmp:?}");
            // Same directory (same volume) as the target.
            assert_eq!(tmp.parent(), path.parent());
        }
    }

    #[test]
    fn concurrent_saves_produce_valid_json() {
        // Serialized writers (AppState::persist holds persist_lock) plus
        // unique temp names: concurrent saves must each land a complete,
        // valid file — never a torn write, never a shared-temp collision.
        // (The lock itself lives in AppState; here we prove the file layer
        // is safe even when the serialization is bypassed.)
        let path = temp_file("concurrent");
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = vec![];
        for i in 0..8u32 {
            let path = path.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let mut cfg = AppConfig::default();
                cfg.selected_layout = "60-ansi".into();
                cfg.profiles.push(Profile {
                    id: format!("racer-{i}"),
                    name: format!("Racer {i}"),
                    layout: "60-ansi".into(),
                    disabled_keys: vec![],
                    builtin: false,
                });
                let data = serde_json::to_vec_pretty(&cfg).unwrap();
                atomic_write(&path, &data).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let raw = fs::read_to_string(&path).unwrap();
        let parsed: AppConfig = serde_json::from_str(&raw).expect("final file must be valid JSON");
        assert!(parsed.profiles.iter().any(|p| p.id.starts_with("racer-")));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_with_file_as_parent_dir_fails() {
        // Deterministic even as root: the settings path's parent is a
        // regular file, so preparing the directory must fail and the error
        // must surface (original-preservation under mid-write failure is
        // covered by `failed_replace_preserves_original` via fault
        // injection).
        let blocker = temp_file("unwritable-blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let path = blocker.join("settings.json");
        let result = save(&path, &AppConfig::default());
        assert!(result.is_err(), "save with file-as-parent must fail");
        let _ = fs::remove_file(&blocker);
    }
}
