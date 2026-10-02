use super::*;

#[test]
fn cloud_worker_has_no_lifecycle_flags() {
    let base = [
        "worker",
        "--server-ref",
        "server",
        "--provider",
        "openai",
        "--provider-model",
        "test",
        "--host",
        "127.0.0.1",
        "--port",
        "8780",
    ];
    assert!(CloudServerArgs::try_parse_from(base).is_ok());
    for flags in [
        vec!["--lazy-load"],
        vec!["--idle-seconds", "30"],
        vec!["--runtime-idle-seconds", "30"],
        vec!["--model-idle-seconds", "0"],
    ] {
        let mut args = base.to_vec();
        args.extend(flags);
        assert!(CloudServerArgs::try_parse_from(args).is_err());
    }
}

#[test]
fn local_worker_requires_lazy_for_images_but_not_chat() {
    for (capability, lazy, accepted) in [
        ("image-generation", false, false),
        ("image-generation", true, true),
        ("chat", false, true),
    ] {
        let mut args = vec![
            "worker",
            "--server-ref",
            "server",
            "--model-ref",
            "model",
            "--capability",
            capability,
            "--host",
            "127.0.0.1",
            "--port",
            "8780",
        ];
        if lazy {
            args.push("--lazy-load");
        }
        let parsed = LocalServerArgs::try_parse_from(args).unwrap();
        let result = parsed.validated_capability();
        assert_eq!(result.is_ok(), accepted);
        if let Err(error) = result {
            assert!(error.to_string().contains("requires --lazy-load"));
        }
    }
}
