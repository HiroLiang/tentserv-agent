//! Standard keychain prompt planning.

use crate::features::auth::domain::{
    KeychainBiometricSupport, KeychainPromptPlan, KeychainPromptPreference,
};
use crate::features::auth::ports::KeychainPromptPlanner;
use crate::foundation::error::KernelResult;
use crate::foundation::platform::{OperatingSystem, PlatformFacts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StdKeychainPromptPlanner {
    biometric_backend_available: bool,
}

impl Default for StdKeychainPromptPlanner {
    fn default() -> Self {
        Self {
            biometric_backend_available: false,
        }
    }
}

impl StdKeychainPromptPlanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_biometric_backend_available(biometric_backend_available: bool) -> Self {
        Self {
            biometric_backend_available,
        }
    }
}

impl KeychainPromptPlanner for StdKeychainPromptPlanner {
    fn plan_prompt(
        &self,
        platform: &PlatformFacts,
        preference: KeychainPromptPreference,
    ) -> KernelResult<KeychainPromptPlan> {
        if preference != KeychainPromptPreference::PreferBiometric {
            return Ok(KeychainPromptPlan {
                requested: preference,
                effective: preference,
                biometric_support: KeychainBiometricSupport::Unknown {
                    reason: "biometric unlock was not requested".to_string(),
                },
            });
        }

        if platform.os != OperatingSystem::Macos {
            return Ok(KeychainPromptPlan {
                requested: preference,
                effective: KeychainPromptPreference::SystemDefault,
                biometric_support: KeychainBiometricSupport::PlatformUnsupported {
                    reason: "biometric keychain unlock is currently planned only for macOS"
                        .to_string(),
                },
            });
        }

        if !self.biometric_backend_available {
            return Ok(KeychainPromptPlan {
                requested: preference,
                effective: KeychainPromptPreference::SystemDefault,
                biometric_support: KeychainBiometricSupport::BackendUnsupported {
                    reason: "the current generic keyring backend does not expose macOS LocalAuthentication or SecAccessControl".to_string(),
                },
            });
        }

        Ok(KeychainPromptPlan {
            requested: preference,
            effective: KeychainPromptPreference::PreferBiometric,
            biometric_support: KeychainBiometricSupport::Supported,
        })
    }
}
