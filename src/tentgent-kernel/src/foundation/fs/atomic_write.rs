use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

pub fn atomic_write(path: &Path, body: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = temporary_path(path, parent);
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(body)?;
        file.sync_all()?;
        replace_file(&temporary, path)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn temporary_path(path: &Path, parent: &Path) -> PathBuf {
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(
        ".{}.{}-{nanos}-{sequence}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("record"),
        std::process::id(),
    ))
}

#[cfg(unix)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    tentgent_platform_fs::replace_file(source, destination)
}

#[cfg(not(any(unix, windows)))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_atomic_write_replaces_existing_file_without_temp_leaks() {
        let root = std::env::temp_dir().join(format!(
            "tentgent-atomic-write-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("state.toml");
        atomic_write(&path, b"state = 'starting'\n").unwrap();
        atomic_write(&path, b"state = 'ready'\n").unwrap();
        atomic_write(&path, b"state = 'closing'\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "state = 'closing'\n");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
