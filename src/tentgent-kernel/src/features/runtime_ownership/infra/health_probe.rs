use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use serde::Deserialize;

use crate::{
    features::runtime_ownership::{
        RuntimeGenerationHealth, RuntimeGenerationHealthProbe, RuntimeGenerationRecord,
    },
    foundation::error::KernelResult,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct StdRuntimeGenerationHealthProbe;

impl RuntimeGenerationHealthProbe for StdRuntimeGenerationHealthProbe {
    fn probe_generation_health(
        &self,
        record: &RuntimeGenerationRecord,
    ) -> KernelResult<RuntimeGenerationHealth> {
        let target = record
            .endpoint
            .as_ref()
            .map(|endpoint| (endpoint.host.as_str(), endpoint.port))
            .or_else(|| {
                record
                    .launch_target
                    .as_ref()
                    .map(|target| (target.host.as_str(), target.port))
            });
        let Some((host, port)) = target else {
            return Ok(RuntimeGenerationHealth::Unavailable {
                description: "runtime generation has no health endpoint".to_string(),
            });
        };
        let target = format!("{host}:{port}");
        let payload = match read_health(&target) {
            Ok(payload) => payload,
            Err(description) => {
                return Ok(RuntimeGenerationHealth::Unavailable { description });
            }
        };
        if let Some(endpoint) = &record.endpoint {
            if payload.pid != endpoint.pid {
                return Ok(RuntimeGenerationHealth::Mismatch {
                    description: format!(
                        "health endpoint reported pid {}, expected {}",
                        payload.pid, endpoint.pid
                    ),
                });
            }
        }
        if payload.process_token.as_deref() != Some(record.process_token.as_str()) {
            return Ok(RuntimeGenerationHealth::Mismatch {
                description: "health endpoint process token does not match ownership record"
                    .to_string(),
            });
        }
        if payload.runtime.capability != record.identity.capability().as_str()
            || payload.runtime.model_ref.as_deref() != record.identity.model_ref()
        {
            return Ok(RuntimeGenerationHealth::Mismatch {
                description: "health endpoint runtime identity does not match ownership record"
                    .to_string(),
            });
        }
        Ok(RuntimeGenerationHealth::Matching {
            status: payload.status,
            endpoint: crate::features::runtime_ownership::RuntimeGenerationEndpoint {
                host: host.to_string(),
                port,
                pid: payload.pid,
                process_token: record.process_token.clone(),
            },
        })
    }
}

#[derive(Deserialize)]
struct HealthPayload {
    status: String,
    pid: u32,
    process_token: Option<String>,
    runtime: HealthRuntime,
}

#[derive(Deserialize)]
struct HealthRuntime {
    capability: String,
    model_ref: Option<String>,
}

fn read_health(target: &str) -> Result<HealthPayload, String> {
    let mut addrs = target
        .to_socket_addrs()
        .map_err(|error| format!("resolve health endpoint failed: {error}"))?;
    let Some(address) = addrs.next() else {
        return Err("health endpoint did not resolve".to_string());
    };
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
        .map_err(|error| format!("connect health endpoint failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| format!("set health read timeout failed: {error}"))?;
    stream
        .write_all(
            format!("GET /healthz HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .map_err(|error| format!("write health request failed: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read health response failed: {error}"))?;
    if !response.starts_with("HTTP/1.1 200") && !response.starts_with("HTTP/1.0 200") {
        return Err("health endpoint returned a non-success response".to_string());
    }
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .ok_or_else(|| "health endpoint returned an invalid response".to_string())?;
    serde_json::from_str(body).map_err(|error| format!("decode health response failed: {error}"))
}
