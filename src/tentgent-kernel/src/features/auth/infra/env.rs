//! Standard environment-secret probe.

use std::env;
use std::path::{Path, PathBuf};

use crate::features::auth::domain::{
    AuthEnvLoadPolicy, AuthEnvSecretMaterial, AuthEnvSecretOrigin, Provider,
};
use crate::features::auth::ports::AuthEnvSecretProbe;
use crate::foundation::error::{KernelError, KernelResult};
use zeroize::Zeroize;

#[derive(Debug, Clone, Copy, Default)]
pub struct StdAuthEnvSecretProbe;

impl AuthEnvSecretProbe for StdAuthEnvSecretProbe {
    fn probe_env_secret(
        &self,
        provider: Provider,
        policy: AuthEnvLoadPolicy,
    ) -> KernelResult<Option<AuthEnvSecretMaterial>> {
        let env_var = provider.env_var();
        match policy {
            AuthEnvLoadPolicy::ProcessOnly => process_env_secret(provider, env_var),
            AuthEnvLoadPolicy::CwdDotenvOverride => {
                if let Some(path) = find_cwd_dotenv()? {
                    match dotenv_secret(&path, env_var)? {
                        DotenvSecret::Present(Some(secret)) => {
                            return Ok(Some(env_secret(
                                provider,
                                env_var,
                                secret,
                                AuthEnvSecretOrigin::DotenvFile { path },
                            )));
                        }
                        DotenvSecret::Present(None) => return Ok(None),
                        DotenvSecret::Missing => {}
                    }
                }
                process_env_secret(provider, env_var)
            }
            AuthEnvLoadPolicy::ExplicitDotenvOverride { path } => {
                match dotenv_secret(&path, env_var)? {
                    DotenvSecret::Present(Some(secret)) => {
                        return Ok(Some(env_secret(
                            provider,
                            env_var,
                            secret,
                            AuthEnvSecretOrigin::DotenvFile { path },
                        )));
                    }
                    DotenvSecret::Present(None) => return Ok(None),
                    DotenvSecret::Missing => {}
                }
                process_env_secret(provider, env_var)
            }
        }
    }
}

fn process_env_secret(
    provider: Provider,
    env_var: &str,
) -> KernelResult<Option<AuthEnvSecretMaterial>> {
    Ok(env::var(env_var)
        .ok()
        .and_then(clean_owned)
        .map(|secret| env_secret(provider, env_var, secret, AuthEnvSecretOrigin::ProcessEnv)))
}

enum DotenvSecret {
    Missing,
    Present(Option<String>),
}

fn dotenv_secret(path: &Path, env_var: &str) -> KernelResult<DotenvSecret> {
    if !path.is_file() {
        return Ok(DotenvSecret::Missing);
    }

    let iter = dotenvy::from_path_iter(path).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to read auth dotenv file `{}`: {err}",
            path.display()
        ))
    })?;

    let mut found = DotenvSecret::Missing;
    for item in iter {
        let (key, value) = item.map_err(|err| {
            KernelError::RuntimeStateUnavailable(format!(
                "failed to parse auth dotenv file `{}`: {err}",
                path.display()
            ))
        })?;
        if key == env_var {
            found = DotenvSecret::Present(clean_owned(value));
        }
    }

    Ok(found)
}

fn find_cwd_dotenv() -> KernelResult<Option<PathBuf>> {
    let mut dir = env::current_dir().map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to resolve current directory for auth dotenv probe: {err}"
        ))
    })?;

    loop {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            return Ok(Some(candidate));
        }

        if !dir.pop() {
            return Ok(None);
        }
    }
}

fn env_secret(
    provider: Provider,
    env_var: &str,
    secret: String,
    origin: AuthEnvSecretOrigin,
) -> AuthEnvSecretMaterial {
    AuthEnvSecretMaterial::new(provider, env_var, origin, secret)
}

fn clean_owned(mut value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        value.zeroize();
        None
    } else if trimmed.len() == value.len() {
        Some(value)
    } else {
        let trimmed = trimmed.to_string();
        value.zeroize();
        Some(trimmed)
    }
}
