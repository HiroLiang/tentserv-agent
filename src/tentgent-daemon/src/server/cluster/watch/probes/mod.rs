#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod metadata;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub(super) use linux::PlatformDefinitionRevisionProbe;
#[cfg(target_os = "macos")]
pub(super) use macos::PlatformDefinitionRevisionProbe;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(super) use metadata::MetadataDefinitionRevisionProbe as PlatformDefinitionRevisionProbe;
#[cfg(target_os = "windows")]
pub(super) use windows::PlatformDefinitionRevisionProbe;
