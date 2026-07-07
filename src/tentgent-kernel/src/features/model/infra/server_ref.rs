use std::fs;

use serde::Deserialize;

use crate::features::cluster::domain::{ClusterDefinition, ClusterRouteTarget};
use crate::features::model::domain::ModelRef;
use crate::features::model::ports::ModelServerReferenceProbe;
use crate::features::server::domain::ServerCapability;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::error::{model_store_error, path_error};

/// Reads stored server specs to find model removal blockers.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileModelServerReferenceProbe;

impl ModelServerReferenceProbe for FileModelServerReferenceProbe {
    fn server_refs_for_model(
        &self,
        layout: &RuntimeLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<String>> {
        let mut refs = server_refs_for_model(layout, model_ref, None)?;
        refs.extend(cluster_refs_for_model(layout, model_ref, None)?);
        refs.sort();
        refs.dedup();
        Ok(refs)
    }

    fn refs_for_model_capability(
        &self,
        layout: &RuntimeLayout,
        model_ref: &ModelRef,
        capability: crate::features::model::domain::ModelCapability,
    ) -> KernelResult<Vec<String>> {
        let server_capability = server_capability_for_model_capability(capability);
        let mut refs = Vec::new();
        for capability in server_capability {
            refs.extend(server_refs_for_model(layout, model_ref, Some(capability))?);
        }
        refs.extend(cluster_refs_for_model(layout, model_ref, Some(capability))?);
        refs.sort();
        refs.dedup();
        Ok(refs)
    }
}

#[derive(Debug, Deserialize)]
struct StoredServerSpec {
    short_ref: String,
    #[serde(default)]
    model_ref: Option<String>,
    #[serde(default)]
    capability: Option<ServerCapability>,
}

fn server_refs_for_model(
    layout: &RuntimeLayout,
    model_ref: &ModelRef,
    capability: Option<ServerCapability>,
) -> KernelResult<Vec<String>> {
    let mut refs = Vec::new();
    if !layout.servers_dir.exists() {
        return Ok(refs);
    }

    for entry in fs::read_dir(&layout.servers_dir)
        .map_err(|err| path_error("read servers directory", &layout.servers_dir, err))?
    {
        let entry = entry.map_err(|err| {
            model_store_error(format!(
                "read entry in servers directory `{}` failed: {err}",
                layout.servers_dir.display()
            ))
        })?;
        let file_type = entry
            .file_type()
            .map_err(|err| path_error("read server entry type", entry.path().as_path(), err))?;
        if !file_type.is_dir() {
            continue;
        }

        let spec_path = entry.path().join("server.toml");
        if !spec_path.exists() {
            continue;
        }

        let body = fs::read_to_string(&spec_path)
            .map_err(|err| path_error("read server spec", &spec_path, err))?;
        let spec: StoredServerSpec = toml::from_str(&body).map_err(|err| {
            model_store_error(format!(
                "parse server spec `{}` failed: {err}",
                spec_path.display()
            ))
        })?;

        let Some(spec_model_ref) = spec.model_ref else {
            continue;
        };
        if !(spec_model_ref == model_ref.as_str()
            || model_ref.as_str().starts_with(&spec_model_ref))
        {
            continue;
        }
        if capability
            .is_some_and(|capability| spec.capability.is_some_and(|value| value != capability))
        {
            continue;
        }

        refs.push(format!("server-spec {}", spec.short_ref));
    }

    Ok(refs)
}

fn cluster_refs_for_model(
    layout: &RuntimeLayout,
    model_ref: &ModelRef,
    capability: Option<crate::features::model::domain::ModelCapability>,
) -> KernelResult<Vec<String>> {
    let mut refs = Vec::new();
    let clusters_dir = layout
        .home_dir
        .join(crate::features::cluster::domain::CLUSTERS_DIRNAME);
    if !clusters_dir.exists() {
        return Ok(refs);
    }

    for entry in fs::read_dir(&clusters_dir)
        .map_err(|err| path_error("read clusters directory", &clusters_dir, err))?
    {
        let entry = entry.map_err(|err| {
            model_store_error(format!(
                "read entry in clusters directory `{}` failed: {err}",
                clusters_dir.display()
            ))
        })?;
        let file_type = entry
            .file_type()
            .map_err(|err| path_error("read cluster entry type", entry.path().as_path(), err))?;
        if !file_type.is_dir() {
            continue;
        }

        let definition_path = entry
            .path()
            .join(crate::features::cluster::domain::CLUSTER_DEFINITION_FILENAME);
        if !definition_path.exists() {
            continue;
        }

        let body = fs::read_to_string(&definition_path)
            .map_err(|err| path_error("read cluster definition", &definition_path, err))?;
        let definition: ClusterDefinition = toml::from_str(&body).map_err(|err| {
            model_store_error(format!(
                "parse cluster definition `{}` failed: {err}",
                definition_path.display()
            ))
        })?;

        for (route, target) in definition.routes {
            let ClusterRouteTarget::LocalModel {
                model_ref: route_model_ref,
                ..
            } = target
            else {
                continue;
            };
            if route_model_ref != *model_ref {
                continue;
            }
            if capability.is_some_and(|capability| route.model_capability() != capability) {
                continue;
            }
            refs.push(format!(
                "cluster-route {}:{}",
                definition.cluster_ref, route
            ));
        }
    }

    Ok(refs)
}

fn server_capability_for_model_capability(
    capability: crate::features::model::domain::ModelCapability,
) -> Vec<ServerCapability> {
    use crate::features::model::domain::ModelCapability;

    match capability {
        ModelCapability::AudioSpeech => vec![ServerCapability::AudioSpeech],
        ModelCapability::AudioTranscription => vec![ServerCapability::AudioTranscription],
        ModelCapability::Chat => vec![ServerCapability::Chat],
        ModelCapability::Embedding => vec![ServerCapability::Embedding],
        ModelCapability::ImageGeneration => vec![ServerCapability::ImageGeneration],
        ModelCapability::Rerank => vec![ServerCapability::Rerank],
        ModelCapability::VideoUnderstanding => vec![ServerCapability::VideoUnderstanding],
        ModelCapability::VisionChat => vec![ServerCapability::VisionChat],
    }
}
