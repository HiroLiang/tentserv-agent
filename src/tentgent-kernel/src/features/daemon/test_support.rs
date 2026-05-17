use crate::features::daemon::ports::DaemonPortFuture;

pub(crate) fn successful_healthz_probe<'a>(daemon_url: &'a str) -> DaemonPortFuture<'a, ()> {
    Box::pin(async move {
        assert_http_daemon_url(daemon_url);
        Ok(())
    })
}

pub(crate) fn assert_http_daemon_url(daemon_url: &str) {
    assert!(daemon_url.starts_with("http://"));
}
