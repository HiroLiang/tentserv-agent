use super::*;

pub(super) async fn spawn_test_server(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    (http_url_from_host_port("127.0.0.1", port), task)
}

pub(super) fn test_runtime_layout(
    label: &str,
) -> tentgent_kernel::foundation::layout::RuntimeLayout {
    let home = std::env::temp_dir().join(format!(
        "tentgent-local-server-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home),
            data_root_dir: None,
        })
        .expect("layout")
}

pub(super) fn assert_embedding_values(value: &Value, expected: &[f64]) {
    let values = value.as_array().expect("embedding array");
    assert_eq!(values.len(), expected.len());
    for (value, expected) in values.iter().zip(expected) {
        let value = value.as_f64().expect("embedding float");
        assert!((value - expected).abs() < 0.00001);
    }
}
