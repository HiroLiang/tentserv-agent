use std::{
    net::SocketAddr,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

use miette::{miette, IntoDiagnostic};
use tentgent_core::daemon::DaemonInspection;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Notify,
};

use crate::{
    http::{read_request, write_response, HttpAfterWriteAction},
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
            let (stream, peer_addr) = tokio::select! {
                _ = state.wait_for_shutdown() => return Ok(()),
                accepted = self.listener.accept() => accepted.into_diagnostic()?,
            };
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
    shutdown: Arc<Notify>,
    shutdown_requested: Arc<AtomicBool>,
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
            shutdown: Arc::new(Notify::new()),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
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

    pub(crate) fn request_shutdown(&self) {
        if !self.shutdown_requested.swap(true, Ordering::SeqCst) {
            self.shutdown.notify_waiters();
        }
    }

    pub(crate) fn shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::SeqCst)
    }

    async fn wait_for_shutdown(&self) {
        if self.shutdown_requested() {
            return;
        }
        self.shutdown.notified().await;
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
    let after_write = response.after_write;
    eprintln!(
        "tentgent-http request peer={} method={} path={} status={} elapsed_ms={}",
        peer_addr,
        request.method_label(),
        request.path_label(),
        response.status_code,
        started.elapsed().as_millis()
    );
    write_response(&mut stream, response).await?;
    if after_write == Some(HttpAfterWriteAction::RequestDaemonShutdown) {
        state.request_shutdown();
    }
    Ok(())
}
