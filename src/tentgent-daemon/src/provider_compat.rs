use tentgent_kernel::{
    features::{
        auth::domain::Provider,
        cloud::domain::{provider_supports, CloudEndpointCapability},
    },
    foundation::error::KernelError,
};

use crate::transport::rest::error::RestError;

pub(crate) const UNSUPPORTED_PROVIDER_FIELD: &str = "unsupported_provider_field";
pub(crate) const UNSUPPORTED_PROVIDER_CONTENT: &str = "unsupported_provider_content";
pub(crate) const UNSUPPORTED_PROVIDER_OPERATION: &str = "unsupported_provider_operation";
pub(crate) const UNSUPPORTED_PROVIDER_CAPABILITY: &str = "unsupported_provider_capability";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderCompatRejection {
    code: &'static str,
    message: String,
}

impl ProviderCompatRejection {
    pub(crate) fn unsupported_field(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_FIELD, message)
    }

    pub(crate) fn unsupported_content(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_CONTENT, message)
    }

    pub(crate) fn unsupported_operation(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_OPERATION, message)
    }

    pub(crate) fn unsupported_capability(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_CAPABILITY, message)
    }

    pub(crate) fn into_parts(self) -> (&'static str, String) {
        (self.code, self.message)
    }

    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<ProviderCompatRejection> for RestError {
    fn from(rejection: ProviderCompatRejection) -> Self {
        let (code, message) = rejection.into_parts();
        RestError::bad_request(code, message)
    }
}

pub(crate) fn ensure_provider_capability(
    provider: Provider,
    capability: CloudEndpointCapability,
) -> Result<(), ProviderCompatRejection> {
    if provider_supports(provider, capability) {
        return Ok(());
    }
    Err(ProviderCompatRejection::unsupported_capability(
        provider_capability_message(provider, capability),
    ))
}

pub(crate) fn map_provider_kernel_error(
    fallback_code: impl Into<String>,
    error: KernelError,
) -> RestError {
    match error {
        KernelError::UnsupportedTarget(message) => {
            ProviderCompatRejection::unsupported_capability(message).into()
        }
        other => RestError::kernel(fallback_code, other),
    }
}

fn provider_capability_message(provider: Provider, capability: CloudEndpointCapability) -> String {
    format!(
        "{} does not support cloud {} through Tentgent yet",
        provider.display_name(),
        capability.as_str()
    )
}
