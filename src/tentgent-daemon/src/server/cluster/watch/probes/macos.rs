use super::super::super::error::ClusterServerError;
use super::super::{super::cache::ClusterDefinitionCache, port::DefinitionRevisionProbe};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PlatformDefinitionRevisionProbe;

impl DefinitionRevisionProbe for PlatformDefinitionRevisionProbe {
    fn refresh(
        &self,
        cache: &ClusterDefinitionCache,
        force_hash: bool,
    ) -> Result<String, ClusterServerError> {
        super::metadata::MetadataDefinitionRevisionProbe.refresh(cache, force_hash)
    }
}
