//! Ensure runtime layout directories exist.

use crate::foundation::error::KernelResult;
use crate::foundation::layout::domain::{LayoutResolveMode, RuntimeLayout};
use crate::foundation::layout::resolver::RuntimeLayoutResolver;

pub struct EnsureRuntimeLayout<'a, R> {
    pub resolver: &'a R,
}

impl<'a, R> EnsureRuntimeLayout<'a, R> {
    pub fn new(resolver: &'a R) -> Self {
        Self { resolver }
    }
}

impl<R> EnsureRuntimeLayout<'_, R>
where
    R: RuntimeLayoutResolver,
{
    pub fn run(&self) -> KernelResult<RuntimeLayout> {
        self.resolver
            .resolve_runtime_layout(LayoutResolveMode::Create)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::foundation::error::KernelResult;
    use crate::foundation::layout::domain::{LayoutResolveMode, RuntimeLayout};
    use crate::foundation::layout::resolver::RuntimeLayoutResolver;

    use super::EnsureRuntimeLayout;

    #[derive(Debug)]
    struct FakeLayoutResolver {
        layout: RuntimeLayout,
    }

    impl RuntimeLayoutResolver for FakeLayoutResolver {
        fn resolve_runtime_layout(&self, mode: LayoutResolveMode) -> KernelResult<RuntimeLayout> {
            assert_eq!(mode, LayoutResolveMode::Create);
            Ok(self.layout.clone())
        }
    }

    #[test]
    fn resolves_create_layout() {
        let layout = fixture_layout("/tmp/tentgent-layout-ensure");
        let resolver = FakeLayoutResolver {
            layout: layout.clone(),
        };

        let output = EnsureRuntimeLayout::new(&resolver)
            .run()
            .expect("ensure layout");

        assert_eq!(output, layout);
    }

    fn fixture_layout(root: &str) -> RuntimeLayout {
        let home_dir = PathBuf::from(root);
        RuntimeLayout {
            config_path: home_dir.join("config.toml"),
            models_dir: home_dir.join("models"),
            adapters_dir: home_dir.join("adapters"),
            datasets_dir: home_dir.join("datasets"),
            sessions_dir: home_dir.join("sessions"),
            servers_dir: home_dir.join("servers"),
            train_dir: home_dir.join("train"),
            cache_dir: home_dir.join("cache"),
            runtime_dir: home_dir.join("runtime"),
            logs_dir: home_dir.join("logs"),
            locks_dir: home_dir.join("locks"),
            python_env_dir: home_dir.join("runtime/python-env"),
            bootstrap_dir: home_dir.join("runtime/bootstrap"),
            bootstrap_uv_dir: home_dir.join("runtime/bootstrap/uv"),
            bootstrap_uv_cache_dir: home_dir.join("runtime/bootstrap/uv-cache"),
            capability_manifest_path: home_dir.join("runtime/capabilities.toml"),
            home_dir,
        }
    }
}
