use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use serde::Deserialize;

use crate::{
    features::server::{
        domain::{ServerProcessIdentityStatus, ServerRefSelector, ServerStoreLayout},
        ports::{ServerCatalogStore, ServerProcessIdentityProbe},
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

use super::FileServerCatalogStore;

#[derive(Debug, Clone, Copy, Default)]
pub struct StdServerProcessIdentityProbe;

impl ServerProcessIdentityProbe for StdServerProcessIdentityProbe {
    fn probe_process_identity(
        &self,
        layout: &RuntimeLayout,
        server_ref: &str,
        expected_pid: u32,
        expected_token: &str,
    ) -> KernelResult<ServerProcessIdentityStatus> {
        let selector = match ServerRefSelector::parse(server_ref) {
            Ok(selector) => selector,
            Err(error) => {
                return Ok(ServerProcessIdentityStatus::Unavailable {
                    description: format!("invalid server reference in ownership state: {error}"),
                });
            }
        };
        let store = ServerStoreLayout::from_home_and_servers_dir(
            layout.home_dir.clone(),
            layout.servers_dir.clone(),
        );
        let inspection = match FileServerCatalogStore::default().inspect_server(&store, &selector) {
            Ok(inspection) => inspection,
            Err(error) => {
                return Ok(ServerProcessIdentityStatus::Unavailable {
                    description: format!("inspect owning server failed: {error}"),
                });
            }
        };
        let Some(process) = inspection.process.as_ref() else {
            return Ok(ServerProcessIdentityStatus::Stopped);
        };
        if !inspection.running {
            return Ok(ServerProcessIdentityStatus::Stopped);
        }
        if process.pid != expected_pid {
            return Ok(ServerProcessIdentityStatus::Mismatch {
                description: format!(
                    "server process metadata reports pid {}, expected {expected_pid}",
                    process.pid
                ),
            });
        }
        if process.process_token.as_deref() != Some(expected_token) {
            return Ok(ServerProcessIdentityStatus::Mismatch {
                description: "server process metadata token does not match ownership state"
                    .to_string(),
            });
        }

        let health = match read_health(&inspection.spec.host, inspection.effective_port()) {
            Ok(health) => health,
            Err(description) => {
                return Ok(ServerProcessIdentityStatus::Unavailable { description });
            }
        };
        if health.server_ref != inspection.spec.server_ref.as_str()
            || health.process_token.as_deref() != Some(expected_token)
            || health.runtime_home.as_deref() != Some(layout.home_dir.to_string_lossy().as_ref())
        {
            return Ok(ServerProcessIdentityStatus::Mismatch {
                description: "server health identity does not match process metadata".to_string(),
            });
        }
        Ok(ServerProcessIdentityStatus::Matching)
    }
}

#[derive(Debug, Deserialize)]
struct ServerHealthIdentity {
    server_ref: String,
    process_token: Option<String>,
    runtime_home: Option<String>,
}

fn read_health(host: &str, port: u16) -> Result<ServerHealthIdentity, String> {
    let target = socket_addr_text(host, port);
    let mut addrs = target
        .to_socket_addrs()
        .map_err(|error| format!("resolve server health endpoint failed: {error}"))?;
    let Some(address) = addrs.next() else {
        return Err("server health endpoint did not resolve".to_string());
    };
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
        .map_err(|error| format!("connect server health endpoint failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| format!("set server health read timeout failed: {error}"))?;
    stream
        .write_all(
            format!("GET /healthz HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .map_err(|error| format!("write server health request failed: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read server health response failed: {error}"))?;
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .ok_or_else(|| "server health endpoint returned an invalid response".to_string())?;
    serde_json::from_str(body)
        .map_err(|error| format!("decode server health response failed: {error}"))
}

fn socket_addr_text(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use crate::{
        features::server::{
            domain::ServerProcessIdentityStatus, ports::ServerProcessIdentityProbe,
        },
        foundation::layout::RuntimeLayout,
    };

    use super::StdServerProcessIdentityProbe;

    #[test]
    fn process_identity_requires_matching_metadata_and_health_token() {
        let root = std::env::temp_dir().join(format!(
            "tentgent-server-process-identity-{}",
            crate::features::resource_coordination::new_operation_id()
        ));
        let layout = runtime_layout(&root);
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().unwrap().port();
        let server_ref = "a".repeat(64);
        let token = "server-process-token";
        write_server_state(&layout, &server_ref, port, token);
        let home = layout.home_dir.display().to_string();
        let response_ref = server_ref.clone();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept health");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let body = serde_json::json!({
                "server_ref": response_ref,
                "process_token": token,
                "runtime_home": home,
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("health response");
        });
        let status = StdServerProcessIdentityProbe
            .probe_process_identity(&layout, &server_ref, std::process::id(), token)
            .expect("identity probe");
        assert_eq!(status, ServerProcessIdentityStatus::Matching);
        worker.join().expect("health worker");

        let mismatch = StdServerProcessIdentityProbe
            .probe_process_identity(&layout, &server_ref, std::process::id(), "different-token")
            .expect("mismatch probe");
        assert!(matches!(
            mismatch,
            ServerProcessIdentityStatus::Mismatch { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }

    fn write_server_state(layout: &RuntimeLayout, server_ref: &str, port: u16, token: &str) {
        let server_dir = layout.servers_dir.join(server_ref);
        fs::create_dir_all(&server_dir).expect("server dir");
        fs::write(
            server_dir.join("server.toml"),
            format!(
                "server_ref = \"{server_ref}\"\nshort_ref = \"{}\"\nruntime_kind = \"local\"\ncapability = \"chat\"\nmodel_ref = \"{}\"\nhost = \"127.0.0.1\"\nport = {port}\nlazy_load = true\ncreated_at = \"2026-07-14T00:00:00Z\"\n",
                &server_ref[..12],
                "b".repeat(64),
            ),
        )
        .expect("server spec");
        fs::write(
            server_dir.join("process.toml"),
            format!(
                "pid = {}\nprocess_token = \"{token}\"\nlaunch_mode = \"background\"\nstarted_at = \"2026-07-14T00:00:01Z\"\nbound_port = {port}\n",
                std::process::id()
            ),
        )
        .expect("process metadata");
    }

    fn runtime_layout(root: &std::path::Path) -> RuntimeLayout {
        RuntimeLayout {
            home_dir: root.to_path_buf(),
            data_root_dir: root.join("data"),
            models_dir: root.join("models"),
            servers_dir: root.join("servers"),
            adapters_dir: root.join("adapters"),
            datasets_dir: root.join("datasets"),
            sessions_dir: root.join("sessions"),
            train_dir: root.join("train"),
            cache_dir: root.join("cache"),
            runtime_dir: root.join("runtime"),
            logs_dir: root.join("logs"),
            locks_dir: root.join("locks"),
            python_env_dir: root.join("runtime/python"),
            bootstrap_dir: root.join("runtime/bootstrap"),
            bootstrap_uv_dir: root.join("runtime/bootstrap/uv"),
            bootstrap_uv_cache_dir: root.join("runtime/bootstrap/uv-cache"),
            capabilities_path: root.join("runtime/capabilities.toml"),
            auth_metadata_path: root.join("runtime/auth.toml"),
            config_path: root.join("config.toml"),
        }
    }
}
