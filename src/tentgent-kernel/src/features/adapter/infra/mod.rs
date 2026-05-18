//! Standard adapter-store infrastructure implementations.

mod base_index;
mod catalog;
mod content;
mod error;
mod hf_snapshot;
mod identity;
mod index;
mod layout;
mod manifest;
mod metadata;
mod server_ref;
mod staging;

#[cfg(test)]
mod tests;

pub use base_index::FileAdapterBaseIndexStore;
pub use catalog::FileAdapterCatalogStore;
pub use content::FileAdapterContentStore;
pub use hf_snapshot::StdHfAdapterSnapshotFetcher;
pub use identity::StdAdapterIdentityGenerator;
pub use index::FileAdapterSourceIndexStore;
pub use layout::StdAdapterStoreLayoutInitializer;
pub use manifest::StdAdapterManifestBuilder;
pub use metadata::StdAdapterSourceMetadataReader;
pub use server_ref::FileAdapterServerReferenceProbe;
pub use staging::StdAdapterSourceStager;
