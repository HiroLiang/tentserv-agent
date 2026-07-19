use std::net::{TcpListener, ToSocketAddrs};

use crate::foundation::error::{KernelError, KernelResult};

const PORT_SCAN_LIMIT: u16 = 100;

pub(super) fn allocate_bind_port(host: &str, start: u16) -> KernelResult<u16> {
    let max = u32::from(u16::MAX);
    let end = (u32::from(start) + u32::from(PORT_SCAN_LIMIT).saturating_sub(1)).min(max);
    let mut last_error = None;
    for port in u32::from(start)..=end {
        let port = port as u16;
        match ensure_bind_available(host, port) {
            Ok(()) => return Ok(port),
            Err(err) => last_error = Some(err.to_string()),
        }
    }
    Err(runtime_error(format!(
        "no available model runtime bind port on {host} in auto range {start}..={end}{}",
        last_error
            .map(|err| format!("; last error: {err}"))
            .unwrap_or_default()
    )))
}

pub(super) fn socket_addr_text(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn ensure_bind_available(host: &str, port: u16) -> KernelResult<()> {
    let target = socket_addr_text(host, port);
    let mut last_error = None;
    let mut resolved_any = false;
    for addr in target
        .to_socket_addrs()
        .map_err(|err| runtime_error(format!("resolve bind address {target} failed: {err}")))?
    {
        resolved_any = true;
        match TcpListener::bind(addr) {
            Ok(listener) => {
                drop(listener);
                return Ok(());
            }
            Err(err) => last_error = Some(err),
        }
    }
    Err(runtime_error(if resolved_any {
        format!(
            "model runtime bind address {target} is not available: {}",
            last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "unknown bind error".to_string())
        )
    } else {
        format!("model runtime bind address {target} did not resolve to any socket address")
    }))
}

fn runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::RuntimeStateUnavailable(message.into())
}
