//! Standard environment-secret probe.

use std::env;
use std::path::{Path, PathBuf};

use crate::features::auth::domain::{
    normalize_secret_value, AuthEnvLoadPolicy, AuthEnvSecretMaterial, AuthEnvSecretOrigin, Provider,
};
use crate::features::auth::ports::AuthEnvSecretProbe;
use crate::foundation::error::{KernelError, KernelResult};

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
                dotenv_or_process_env_secret(provider, env_var, find_cwd_dotenv()?)
            }
            AuthEnvLoadPolicy::ExplicitDotenvOverride { path } => {
                dotenv_or_process_env_secret(provider, env_var, Some(path))
            }
        }
    }
}

fn dotenv_or_process_env_secret(
    provider: Provider,
    env_var: &str,
    path: Option<PathBuf>,
) -> KernelResult<Option<AuthEnvSecretMaterial>> {
    if let Some(path) = path {
        match dotenv_path_secret(provider, env_var, path)? {
            DotenvPathSecret::Resolved(secret) => return Ok(secret),
            DotenvPathSecret::Missing => {}
        }
    }

    process_env_secret(provider, env_var)
}

enum DotenvPathSecret {
    Missing,
    Resolved(Option<AuthEnvSecretMaterial>),
}

fn dotenv_path_secret(
    provider: Provider,
    env_var: &str,
    path: PathBuf,
) -> KernelResult<DotenvPathSecret> {
    match dotenv_secret(&path, env_var)? {
        DotenvSecret::Present(Some(secret)) => Ok(DotenvPathSecret::Resolved(Some(env_secret(
            provider,
            env_var,
            secret,
            AuthEnvSecretOrigin::DotenvFile { path },
        )))),
        DotenvSecret::Present(None) => Ok(DotenvPathSecret::Resolved(None)),
        DotenvSecret::Missing => Ok(DotenvPathSecret::Missing),
    }
}

fn process_env_secret(
    provider: Provider,
    env_var: &str,
) -> KernelResult<Option<AuthEnvSecretMaterial>> {
    Ok(env::var(env_var)
        .ok()
        .and_then(normalize_secret_value)
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
            found = DotenvSecret::Present(normalize_secret_value(value));
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
