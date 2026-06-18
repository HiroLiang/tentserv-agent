use tentgent_kernel::{
    features::auth::{
        infra::{
            FileAuthMetadataStore, ProcessSessionAuthSecretCache, ReqwestAuthSecretValidator,
            StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
        },
        usecases::{
            AuthSecretResolution, AuthSecretResolutionRequest, AuthSecretResolverUseCase,
            StdAuthSecretMutationUseCase, StdAuthSecretResolverUseCase,
            StdAuthSecretValidationUseCase, StdAuthStatusUseCase,
        },
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

pub struct AuthKernelComponent {
    env_probe: StdAuthEnvSecretProbe,
    keychain_store: SystemKeychainAuthSecretStore,
    metadata_store: FileAuthMetadataStore,
    cache: ProcessSessionAuthSecretCache,
    validator: ReqwestAuthSecretValidator,
}

impl AuthKernelComponent {
    pub fn bootstrap(layout: &RuntimeLayout) -> KernelResult<Self> {
        Ok(Self {
            env_probe: StdAuthEnvSecretProbe,
            keychain_store: SystemKeychainAuthSecretStore::new(),
            metadata_store: FileAuthMetadataStore::from_layout(layout),
            cache: ProcessSessionAuthSecretCache::new(),
            validator: ReqwestAuthSecretValidator::new()?,
        })
    }

    pub fn status_usecase(&self) -> StdAuthStatusUseCase<'_> {
        StdAuthStatusUseCase::new(&self.env_probe, &self.keychain_store, &self.metadata_store)
    }

    pub fn resolver_usecase(&self) -> StdAuthSecretResolverUseCase<'_> {
        StdAuthSecretResolverUseCase::new(
            &self.env_probe,
            &self.keychain_store,
            &self.metadata_store,
            &self.cache,
        )
    }

    pub fn mutation_usecase(&self) -> StdAuthSecretMutationUseCase<'_> {
        StdAuthSecretMutationUseCase::new(&self.keychain_store, &self.metadata_store, &self.cache)
    }

    pub fn validation_usecase(&self) -> StdAuthSecretValidationUseCase<'_> {
        StdAuthSecretValidationUseCase::new(self, &self.validator, &self.metadata_store)
    }
}

impl AuthSecretResolverUseCase for AuthKernelComponent {
    fn resolve_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution> {
        self.resolver_usecase().resolve_secret(request)
    }
}
