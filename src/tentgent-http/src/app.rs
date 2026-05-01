use std::{net::SocketAddr, path::Path, time::Instant};

use miette::{miette, IntoDiagnostic};
use tentgent_core::daemon::DaemonInspection;
use tokio::net::{TcpListener, TcpStream};

use crate::{
    http::{read_request, write_response},
    routes::route_request,
    security::DaemonSecurityConfig,
};

#[derive(Debug)]
pub struct DaemonHttpServer {
    listener: TcpListener,
    host: String,
    port: u16,
}

impl DaemonHttpServer {
    pub async fn bind(host: String, port: u16) -> miette::Result<Self> {
        let listener = TcpListener::bind((host.as_str(), port))
            .await
            .map_err(|err| {
                miette!("failed to bind daemon HTTP listener on {host}:{port}: {err}")
            })?;
        let local_addr = listener.local_addr().into_diagnostic()?;

        Ok(Self {
            listener,
            host,
            port: local_addr.port(),
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn bind_label(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub async fn serve(self, state: DaemonHttpState) -> miette::Result<()> {
        loop {
            let (stream, peer_addr) = self.listener.accept().await.into_diagnostic()?;
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, peer_addr, state).await {
                    eprintln!("tentgent-http connection_error peer={peer_addr} error={error}");
                }
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct DaemonHttpState {
    inspection: DaemonInspection,
    http_client: reqwest::Client,
    security: DaemonSecurityConfig,
}

impl DaemonHttpState {
    pub fn new(inspection: DaemonInspection) -> Self {
        Self::with_security(inspection, DaemonSecurityConfig::disabled())
    }

    pub fn with_security(inspection: DaemonInspection, security: DaemonSecurityConfig) -> Self {
        Self {
            inspection,
            http_client: reqwest::Client::new(),
            security,
        }
    }

    pub(crate) fn home_dir(&self) -> &Path {
        &self.inspection.home_dir
    }

    pub(crate) fn inspection(&self) -> &DaemonInspection {
        &self.inspection
    }

    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    pub(crate) fn security(&self) -> &DaemonSecurityConfig {
        &self.security
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    state: DaemonHttpState,
) -> miette::Result<()> {
    let started = Instant::now();
    let request = read_request(&mut stream).await?;
    let response = route_request(&request, &state).await;
    eprintln!(
        "tentgent-http request peer={} method={} path={} status={} elapsed_ms={}",
        peer_addr,
        request.method_label(),
        request.path_label(),
        response.status_code,
        started.elapsed().as_millis()
    );
    write_response(&mut stream, response).await
}
