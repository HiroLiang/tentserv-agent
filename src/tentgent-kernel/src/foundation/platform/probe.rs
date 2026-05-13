//! Low-level platform probes.

use std::process::Command;

use crate::foundation::error::KernelResult;

use super::domain::{
    Architecture, CpuFacts, CudaFacts, GpuDeviceFacts, GpuFacts, LibcFacts, LibcFamily, MetalFacts,
    OperatingSystem, PlatformFacts,
};

pub trait PlatformProbe {
    fn query_platform_facts(&self) -> KernelResult<PlatformFacts>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StdPlatformProbe;

impl PlatformProbe for StdPlatformProbe {
    fn query_platform_facts(&self) -> KernelResult<PlatformFacts> {
        Ok(PlatformFacts {
            os: detect_os(),
            arch: detect_arch(),
            libc: detect_libc(),
            cpu: detect_cpu(),
            gpu: detect_gpu(),
        })
    }
}

fn detect_os() -> OperatingSystem {
    match std::env::consts::OS {
        "macos" => OperatingSystem::Macos,
        "linux" => OperatingSystem::Linux,
        "windows" => OperatingSystem::Windows,
        other => OperatingSystem::Other(other.to_string()),
    }
}

fn detect_arch() -> Architecture {
    match std::env::consts::ARCH {
        "aarch64" => Architecture::Aarch64,
        "x86_64" => Architecture::X86_64,
        other => Architecture::Other(other.to_string()),
    }
}

fn detect_libc() -> Option<LibcFacts> {
    if cfg!(target_os = "linux") {
        if let Some(version) = command_stdout("getconf", &["GNU_LIBC_VERSION"]) {
            return Some(LibcFacts {
                family: LibcFamily::Glibc,
                version: Some(version),
            });
        }

        if command_stdout("ldd", &["--version"])
            .map(|output| output.to_lowercase().contains("musl"))
            .unwrap_or(false)
        {
            return Some(LibcFacts {
                family: LibcFamily::Musl,
                version: None,
            });
        }

        return Some(LibcFacts {
            family: LibcFamily::Other("unknown".to_string()),
            version: None,
        });
    }

    None
}

fn detect_cpu() -> CpuFacts {
    let mut features = cpu_features();
    features.sort();
    features.dedup();

    CpuFacts {
        vendor: cpu_vendor(),
        brand: cpu_brand(),
        features,
    }
}

fn cpu_vendor() -> Option<String> {
    if cfg!(target_os = "macos") {
        return command_stdout("sysctl", &["-n", "machdep.cpu.vendor"]);
    }

    if cfg!(target_os = "linux") {
        return linux_cpu_info_value("vendor_id");
    }

    None
}

fn cpu_brand() -> Option<String> {
    if cfg!(target_os = "macos") {
        return command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"])
            .or_else(|| command_stdout("sysctl", &["-n", "machdep.cpu.brand"]));
    }

    if cfg!(target_os = "linux") {
        return linux_cpu_info_value("model name")
            .or_else(|| linux_cpu_info_value("Hardware"))
            .or_else(|| linux_cpu_info_value("Processor"));
    }

    None
}

fn cpu_features() -> Vec<String> {
    let mut features = Vec::new();

    if cfg!(target_os = "macos") {
        if let Some(output) = command_stdout("sysctl", &["-n", "machdep.cpu.features"]) {
            features.extend(
                output
                    .split_whitespace()
                    .map(|feature| feature.to_lowercase()),
            );
        }
        if let Some(output) = command_stdout("sysctl", &["-n", "machdep.cpu.leaf7_features"]) {
            features.extend(
                output
                    .split_whitespace()
                    .map(|feature| feature.to_lowercase()),
            );
        }
    }

    if cfg!(target_os = "linux") {
        if let Some(output) =
            linux_cpu_info_value("flags").or_else(|| linux_cpu_info_value("Features"))
        {
            features.extend(
                output
                    .split_whitespace()
                    .map(|feature| feature.to_lowercase()),
            );
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if features.is_empty() {
            features.push("neon".to_string());
        }
    }

    features
}

fn detect_gpu() -> GpuFacts {
    GpuFacts {
        devices: detect_gpu_devices(),
        cuda: detect_cuda(),
        metal: detect_metal(),
    }
}

fn detect_gpu_devices() -> Vec<GpuDeviceFacts> {
    if let Some(output) =
        command_stdout("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"])
    {
        return output
            .lines()
            .filter_map(|line| {
                let name = line.trim();
                (!name.is_empty()).then(|| GpuDeviceFacts {
                    name: Some(name.to_string()),
                    vendor: Some("NVIDIA".to_string()),
                })
            })
            .collect();
    }

    Vec::new()
}

fn detect_cuda() -> Option<CudaFacts> {
    let driver_version = command_stdout(
        "nvidia-smi",
        &["--query-gpu=driver_version", "--format=csv,noheader"],
    )
    .and_then(|output| output.lines().next().map(|line| line.trim().to_string()))
    .filter(|value| !value.is_empty());

    let runtime_version =
        command_stdout("nvcc", &["--version"]).and_then(parse_nvcc_runtime_version);

    let device_count = command_stdout("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"])
        .map(|output| {
            output
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count() as u32
        });

    if driver_version.is_none() && runtime_version.is_none() && device_count.is_none() {
        return None;
    }

    Some(CudaFacts {
        visible: device_count.unwrap_or(0) > 0 || driver_version.is_some(),
        driver_version,
        runtime_version,
        device_count,
    })
}

fn detect_metal() -> Option<MetalFacts> {
    cfg!(target_os = "macos").then_some(MetalFacts { visible: true })
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn linux_cpu_info_value(key: &str) -> Option<String> {
    let contents = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    contents.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then(|| value.trim().to_string())
    })
}

fn parse_nvcc_runtime_version(output: String) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, version) = line.split_once("release ")?;
        let version = version.split(',').next()?.trim();
        (!version.is_empty()).then(|| version.to_string())
    })
}
