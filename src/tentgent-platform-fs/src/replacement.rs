use std::{io, path::Path};

/// Atomically replaces `destination` with a same-filesystem `source` file.
#[cfg(windows)]
pub fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = extended_length_path(source)?;
    let destination = extended_length_path(destination)?;
    // SAFETY: both buffers are NUL-terminated and remain alive for the call.
    // The flags request replacement and synchronous persistence only; neither
    // pointer is retained by Windows after MoveFileExW returns.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn extended_length_path(path: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    const SEPARATOR: u16 = b'\\' as u16;
    const QUESTION: u16 = b'?' as u16;
    const DOT: u16 = b'.' as u16;
    const VERBATIM_PREFIX: [u16; 4] = [SEPARATOR, SEPARATOR, QUESTION, SEPARATOR];
    const DEVICE_PREFIX: [u16; 4] = [SEPARATOR, SEPARATOR, DOT, SEPARATOR];
    const UNC_PREFIX: [u16; 2] = [SEPARATOR, SEPARATOR];
    const VERBATIM_UNC_PREFIX: [u16; 8] = [
        SEPARATOR,
        SEPARATOR,
        QUESTION,
        SEPARATOR,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        SEPARATOR,
    ];

    let absolute = std::path::absolute(path)?;
    let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows path contains a NUL character",
        ));
    }
    if wide.starts_with(&DEVICE_PREFIX) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows device paths are not supported",
        ));
    }

    let mut extended = Vec::with_capacity(wide.len() + VERBATIM_UNC_PREFIX.len() + 1);
    if wide.starts_with(&VERBATIM_PREFIX) {
        extended.extend_from_slice(&wide);
    } else if wide.starts_with(&UNC_PREFIX) {
        extended.extend_from_slice(&VERBATIM_UNC_PREFIX);
        extended.extend_from_slice(&wide[UNC_PREFIX.len()..]);
    } else {
        extended.extend_from_slice(&VERBATIM_PREFIX);
        extended.extend_from_slice(&wide);
    }
    extended.push(0);
    Ok(extended)
}

/// Keeps the crate testable on non-Windows workspace hosts.
#[cfg(not(windows))]
pub fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::*;

    #[test]
    fn replacement_supports_paths_beyond_the_legacy_windows_limit() {
        let root = std::env::temp_dir().join(format!(
            "tentgent-platform-fs-long-path-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let deep = root
            .join(format!("segment-a-{}", "a".repeat(72)))
            .join(format!("segment-b-{}", "b".repeat(72)))
            .join(format!("segment-c-{}", "c".repeat(72)));
        let source = deep.join("source.tmp");
        let destination = deep.join("destination.toml");
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;

            assert!(source.as_os_str().encode_wide().count() > 260);
            assert!(destination.as_os_str().encode_wide().count() > 260);
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(&source, b"state = 'ready'\n").unwrap();

        replace_file(&source, &destination).unwrap();

        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            "state = 'ready'\n"
        );
        assert!(!source.exists());
        let _ = fs::remove_dir_all(root);
    }
}
