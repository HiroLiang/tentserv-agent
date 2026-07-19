use super::super::super::{cache::ClusterDefinitionCache, error::ClusterServerError};
use super::super::port::DefinitionRevisionProbe;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MetadataDefinitionRevisionProbe;

impl DefinitionRevisionProbe for MetadataDefinitionRevisionProbe {
    fn refresh(
        &self,
        cache: &ClusterDefinitionCache,
        force_hash: bool,
    ) -> Result<String, ClusterServerError> {
        cache.refresh(force_hash).map(|snapshot| snapshot.hash)
    }
}
