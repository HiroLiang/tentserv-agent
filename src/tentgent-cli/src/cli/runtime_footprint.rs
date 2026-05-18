use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use tentgent_kernel::{
    features::runtime::{
        domain::PythonRuntimeResolutionInput,
        infra::{StdPythonRuntimeResolver, StdRuntimeStateProbe},
        usecases::{RuntimeStateRequest, RuntimeStateUseCase, StdRuntimeStateUseCase},
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver},
};

use super::display::format_bytes;

const MAX_FOOTPRINT_SCAN_ENTRIES: usize = 50_000;
const MAX_FOOTPRINT_SCAN_MILLIS: u64 = 2_000;
const BOOTSTRAP_UV_CACHE_DIR_ENV: &str = "TENTGENT_BOOTSTRAP_UV_CACHE_DIR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FootprintEntry {
    pub(crate) field: &'static str,
    pub(crate) title: &'static str,
    pub(crate) path: PathBuf,
    state: FootprintState,
    guidance: Option<&'static str>,
}

impl FootprintEntry {
    pub(crate) fn render_value(&self) -> String {
        match &self.state {
            FootprintState::Size { bytes, partial } => {
                let suffix = if *partial { " (partial scan)" } else { "" };
                format!("{}{suffix}: {}", format_bytes(*bytes), self.path.display())
            }
            FootprintState::Missing => format!("missing: {}", self.path.display()),
            FootprintState::Unavailable(error) => {
                format!("unavailable: {error}: {}", self.path.display())
            }
        }
    }

    pub(crate) fn guidance(&self) -> Option<&'static str> {
        self.guidance
    }

    fn new(
        field: &'static str,
        title: &'static str,
        path: PathBuf,
        guidance: Option<&'static str>,
    ) -> Self {
        let state = scan_path(&path, ScanLimits::default());
        Self {
            field,
            title,
            path,
            state,
            guidance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FootprintState {
    Size { bytes: u64, partial: bool },
    Missing,
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScanLimits {
    max_entries: usize,
    max_elapsed: Duration,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_entries: MAX_FOOTPRINT_SCAN_ENTRIES,
            max_elapsed: Duration::from_millis(MAX_FOOTPRINT_SCAN_MILLIS),
        }
    }
}

pub(crate) fn collect_runtime_footprint(
    runtime_home: &Path,
    python_env: Option<&Path>,
) -> Vec<FootprintEntry> {
    let python_env = python_env
        .map(Path::to_path_buf)
        .unwrap_or_else(|| runtime_home.join("runtime/python-env"));
    let bootstrap_root = runtime_home.join("runtime/bootstrap");
    let bootstrap_uv_tool = bootstrap_root.join("uv");
    let bootstrap_uv_cache = resolve_uv_package_cache(runtime_home);

    vec![
        FootprintEntry::new(
            "runtime_home_size",
            "runtime home",
            runtime_home.to_path_buf(),
            None,
        ),
        FootprintEntry::new(
            "python_env_size",
            "managed Python env",
            python_env,
            Some(
                "Required runtime state. Do not remove unless intentionally repairing or reinstalling.",
            ),
        ),
        FootprintEntry::new(
            "bootstrap_size",
            "bootstrap root",
            bootstrap_root,
            Some("Installer bootstrap data. Most users should leave this directory alone."),
        ),
        FootprintEntry::new(
            "bootstrap_uv_tool_size",
            "pinned uv tool cache",
            bootstrap_uv_tool,
            Some("Pinned installer bootstrap tool cache. Usually preserve this directory."),
        ),
        FootprintEntry::new(
            "bootstrap_uv_cache_size",
            "uv package cache",
            bootstrap_uv_cache,
            Some(
                "Safe-to-recreate cache. It may be removed manually when no Tentgent installer/bootstrap process is running.",
            ),
        ),
    ]
}

pub(crate) fn collect_runtime_footprint_best_effort() -> Vec<FootprintEntry> {
    let layout_resolver = StdRuntimeLayoutResolver;
    let runtime_resolver = StdPythonRuntimeResolver;
    let state_probe = StdRuntimeStateProbe;
    let usecase = StdRuntimeStateUseCase::new(&layout_resolver, &runtime_resolver, &state_probe);
    let Ok(result) = usecase.runtime_state(RuntimeStateRequest {
        layout: RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: None,
            data_root_dir: None,
        },
        runtime: PythonRuntimeResolutionInput::default(),
    }) else {
        return Vec::new();
    };
    let python_env = result
        .runtime
        .as_ref()
        .map(|runtime| runtime.env_dir.as_path())
        .unwrap_or(result.state.python_env_dir.as_path());
    collect_runtime_footprint(&result.layout.home_dir, Some(python_env))
}

fn resolve_uv_package_cache(runtime_home: &Path) -> PathBuf {
    match env::var_os(BOOTSTRAP_UV_CACHE_DIR_ENV) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => runtime_home.join("runtime/bootstrap/uv-cache"),
    }
}

fn scan_path(path: &Path, limits: ScanLimits) -> FootprintState {
    if !path.exists() {
        return FootprintState::Missing;
    }

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => return FootprintState::Unavailable(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return FootprintState::Size {
            bytes: 0,
            partial: false,
        };
    }
    if metadata.is_file() {
        return FootprintState::Size {
            bytes: metadata.len(),
            partial: false,
        };
    }
    if !metadata.is_dir() {
        return FootprintState::Size {
            bytes: 0,
            partial: false,
        };
    }

    match scan_dir(path, limits) {
        Ok(scan) => FootprintState::Size {
            bytes: scan.bytes,
            partial: scan.partial,
        },
        Err(error) => FootprintState::Unavailable(error.to_string()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScanResult {
    bytes: u64,
    partial: bool,
}

fn scan_dir(path: &Path, limits: ScanLimits) -> std::io::Result<ScanResult> {
    let started = Instant::now();
    let mut stack = vec![path.to_path_buf()];
    let mut entries_seen = 0usize;
    let mut bytes = 0u64;
    let mut partial = false;

    while let Some(dir) = stack.pop() {
        if started.elapsed() >= limits.max_elapsed {
            partial = true;
            break;
        }

        for entry in fs::read_dir(&dir)? {
            if entries_seen >= limits.max_entries || started.elapsed() >= limits.max_elapsed {
                partial = true;
                break;
            }
            entries_seen += 1;

            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if file_type.is_file() {
                bytes = bytes.saturating_add(entry.metadata()?.len());
            }
        }

        if partial {
            break;
        }
    }

    Ok(ScanResult { bytes, partial })
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io::Write,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn missing_directory_reports_missing() {
        let home = temp_dir("missing");
        let missing = home.join("does-not-exist");

        let state = scan_path(&missing, ScanLimits::default());

        assert_eq!(state, FootprintState::Missing);
    }

    #[test]
    fn nested_regular_files_are_summed() {
        let home = temp_dir("nested");
        write_bytes(&home.join("a.bin"), 512);
        fs::create_dir_all(home.join("child")).expect("create child");
        write_bytes(&home.join("child/b.bin"), 1024);

        let state = scan_path(&home, ScanLimits::default());

        assert_eq!(
            state,
            FootprintState::Size {
                bytes: 1536,
                partial: false,
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_entries_are_skipped() {
        use std::os::unix::fs::symlink;

        let home = temp_dir("symlink");
        let outside = temp_dir("symlink-outside");
        write_bytes(&outside.join("outside.bin"), 4096);
        write_bytes(&home.join("inside.bin"), 128);
        symlink(outside.join("outside.bin"), home.join("link-file")).expect("link file");
        symlink(&outside, home.join("link-dir")).expect("link dir");

        let state = scan_path(&home, ScanLimits::default());

        assert_eq!(
            state,
            FootprintState::Size {
                bytes: 128,
                partial: false,
            }
        );
    }

    #[test]
    fn entry_cap_marks_result_partial() {
        let home = temp_dir("partial");
        write_bytes(&home.join("a.bin"), 512);
        write_bytes(&home.join("b.bin"), 512);

        let state = scan_path(
            &home,
            ScanLimits {
                max_entries: 1,
                max_elapsed: Duration::from_secs(10),
            },
        );

        assert!(matches!(state, FootprintState::Size { partial: true, .. }));
    }

    #[test]
    fn render_value_formats_sizes_and_partial_marker() {
        let entry = FootprintEntry {
            field: "runtime_home_size",
            title: "runtime home",
            path: PathBuf::from("/tmp/tentgent"),
            state: FootprintState::Size {
                bytes: 1536,
                partial: true,
            },
            guidance: None,
        };

        assert_eq!(
            entry.render_value(),
            "1.5 KiB (partial scan): /tmp/tentgent"
        );
    }

    #[test]
    fn collect_uses_runtime_layout_paths() {
        let home = temp_dir("layout");
        let entries = collect_runtime_footprint(&home, None);

        assert_eq!(entries[0].field, "runtime_home_size");
        assert_eq!(entries[1].path, home.join("runtime/python-env"));
        assert_eq!(entries[2].path, home.join("runtime/bootstrap"));
        assert_eq!(entries[3].path, home.join("runtime/bootstrap/uv"));
        assert_eq!(entries[4].path, home.join("runtime/bootstrap/uv-cache"));
    }

    fn temp_dir(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "tentgent-footprint-{name}-{}-{now}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn write_bytes(path: &Path, len: usize) {
        let mut file = File::create(path).expect("create file");
        file.write_all(&vec![b'x'; len]).expect("write file");
    }
}
