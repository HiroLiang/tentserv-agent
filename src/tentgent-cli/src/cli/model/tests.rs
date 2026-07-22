use clap::Parser as _;

use super::*;
use crate::cli::commands::Commands;

#[test]
fn parses_model_pull_revision_command() {
    let cli = Cli::try_parse_from([
        "tentgent",
        "model",
        "pull",
        "org/model",
        "--revision",
        "main",
    ])
    .expect("parse model pull");

    match cli.command {
        Commands::Model {
            action:
                ModelCommands::Pull {
                    repo_id,
                    revision,
                    capability,
                },
        } => {
            assert_eq!(repo_id, "org/model");
            assert_eq!(revision.as_deref(), Some("main"));
            assert_eq!(capability, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_model_add_capability_command() {
    let cli = Cli::try_parse_from([
        "tentgent",
        "model",
        "add",
        "/tmp/model",
        "--capability",
        "embedding",
    ])
    .expect("parse model add");

    match cli.command {
        Commands::Model {
            action: ModelCommands::Add { path, capability },
        } => {
            assert_eq!(path, std::path::PathBuf::from("/tmp/model"));
            assert_eq!(capability, Some(ModelCapability::Embedding));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_model_pull_capability_command() {
    let cli = Cli::try_parse_from([
        "tentgent",
        "model",
        "pull",
        "org/model",
        "--capability",
        "rerank",
        "--revision",
        "main",
    ])
    .expect("parse model pull");

    match cli.command {
        Commands::Model {
            action:
                ModelCommands::Pull {
                    repo_id,
                    revision,
                    capability,
                },
        } => {
            assert_eq!(repo_id, "org/model");
            assert_eq!(revision.as_deref(), Some("main"));
            assert_eq!(capability, Some(ModelCapability::Rerank));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_model_catalog_filter_command() {
    let cli = Cli::try_parse_from([
        "tentgent",
        "model",
        "catalog",
        "--capability",
        "chat",
        "--publisher",
        "Qwen",
        "--support-level",
        "catalog-known",
        "--local",
        "--query",
        "qwen3",
    ])
    .expect("parse model catalog");

    match cli.command {
        Commands::Model {
            action:
                ModelCommands::Catalog {
                    capability,
                    publisher,
                    support_level,
                    local,
                    query,
                },
        } => {
            assert_eq!(capability, Some(ModelCapability::Chat));
            assert_eq!(publisher.as_deref(), Some("Qwen"));
            assert_eq!(support_level.as_deref(), Some("catalog-known"));
            assert!(local);
            assert_eq!(query.as_deref(), Some("qwen3"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn model_catalog_help_mentions_filters() {
    let mut root = Cli::command();
    let model = root.find_subcommand_mut("model").expect("model command");
    let catalog = model
        .find_subcommand_mut("catalog")
        .expect("catalog command");
    let mut help = Vec::new();
    catalog
        .write_long_help(&mut help)
        .expect("write catalog help");
    let help = String::from_utf8(help).expect("utf8 help");

    assert!(help.contains("--capability"));
    assert!(help.contains("--publisher"));
    assert!(help.contains("--support-level"));
    assert!(help.contains("--local"));
    assert!(help.contains("--query"));
}

#[test]
fn parses_model_set_capability_command() {
    let cli = Cli::try_parse_from(["tentgent", "model", "set-capability", "abc123", "embedding"])
        .expect("parse model set-capability");

    match cli.command {
        Commands::Model {
            action:
                ModelCommands::SetCapability {
                    reference,
                    capability,
                },
        } => {
            assert_eq!(reference, "abc123");
            assert_eq!(capability, ModelCapability::Embedding);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_model_capability_show_command() {
    let cli = Cli::try_parse_from(["tentgent", "model", "capability", "show", "abc123"])
        .expect("parse model capability show");

    match cli.command {
        Commands::Model {
            action:
                ModelCommands::Capability {
                    action: ModelCapabilityCommands::Show { reference },
                },
        } => {
            assert_eq!(reference, "abc123");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_model_capability_set_command_with_multiple_values() {
    let cli = Cli::try_parse_from([
        "tentgent",
        "model",
        "capability",
        "set",
        "abc123",
        "chat",
        "vision-chat",
    ])
    .expect("parse model capability set");

    match cli.command {
        Commands::Model {
            action:
                ModelCommands::Capability {
                    action:
                        ModelCapabilityCommands::Set {
                            reference,
                            capabilities,
                        },
                },
        } => {
            assert_eq!(reference, "abc123");
            assert_eq!(
                capabilities,
                vec![ModelCapability::Chat, ModelCapability::VisionChat]
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_model_capability_add_and_remove_commands() {
    let add = Cli::try_parse_from([
        "tentgent",
        "model",
        "capability",
        "add",
        "abc123",
        "embedding",
    ])
    .expect("parse model capability add");
    match add.command {
        Commands::Model {
            action:
                ModelCommands::Capability {
                    action:
                        ModelCapabilityCommands::Add {
                            reference,
                            capabilities,
                        },
                },
        } => {
            assert_eq!(reference, "abc123");
            assert_eq!(capabilities, vec![ModelCapability::Embedding]);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let remove = Cli::try_parse_from(["tentgent", "model", "capability", "rm", "abc123", "chat"])
        .expect("parse model capability remove alias");
    match remove.command {
        Commands::Model {
            action:
                ModelCommands::Capability {
                    action:
                        ModelCapabilityCommands::Remove {
                            reference,
                            capabilities,
                        },
                },
        } => {
            assert_eq!(reference, "abc123");
            assert_eq!(capabilities, vec![ModelCapability::Chat]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_model_capability_proofs_and_verify_commands() {
    let proofs = Cli::try_parse_from(["tentgent", "model", "capability", "proofs", "abc123"])
        .expect("parse model capability proofs");
    match proofs.command {
        Commands::Model {
            action:
                ModelCommands::Capability {
                    action: ModelCapabilityCommands::Proofs { reference },
                },
        } => assert_eq!(reference, "abc123"),
        other => panic!("unexpected command: {other:?}"),
    }

    let verify = Cli::try_parse_from([
        "tentgent",
        "model",
        "capability",
        "verify",
        "abc123",
        "vision-chat",
    ])
    .expect("parse model capability verify");
    match verify.command {
        Commands::Model {
            action:
                ModelCommands::Capability {
                    action:
                        ModelCapabilityCommands::Verify {
                            reference,
                            capability,
                        },
                },
        } => {
            assert_eq!(reference, "abc123");
            assert_eq!(capability, ModelCapability::VisionChat);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let clear = Cli::try_parse_from([
        "tentgent",
        "model",
        "capability",
        "proof",
        "clear",
        "abc123",
        "chat",
    ])
    .expect("parse model capability proof clear");
    match clear.command {
        Commands::Model {
            action:
                ModelCommands::Capability {
                    action:
                        ModelCapabilityCommands::Proof {
                            action:
                                ModelCapabilityProofCommands::Clear {
                                    reference,
                                    capability,
                                },
                        },
                },
        } => {
            assert_eq!(reference, "abc123");
            assert_eq!(capability, ModelCapability::Chat);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_media_model_capability_values_as_metadata() {
    let cli = Cli::try_parse_from([
        "tentgent",
        "model",
        "pull",
        "org/whisper",
        "--capability",
        "audio-transcription",
    ])
    .expect("parse media model capability");

    match cli.command {
        Commands::Model {
            action:
                ModelCommands::Pull {
                    repo_id,
                    capability,
                    ..
                },
        } => {
            assert_eq!(repo_id, "org/whisper");
            assert_eq!(capability, Some(ModelCapability::AudioTranscription));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn model_set_capability_rejects_unknown_cli_value() {
    let err = Cli::try_parse_from(["tentgent", "model", "set-capability", "abc123", "audio"])
        .expect_err("parse error");

    assert!(err.to_string().contains("unsupported model capability"));
}

#[test]
fn model_capability_set_rejects_unknown_cli_value() {
    let err = Cli::try_parse_from(["tentgent", "model", "capability", "set", "abc123", "audio"])
        .expect_err("parse error");

    assert!(err.to_string().contains("unsupported model capability"));
}

#[test]
fn model_capability_rejects_unknown_cli_value() {
    let err = Cli::try_parse_from([
        "tentgent",
        "model",
        "pull",
        "org/model",
        "--capability",
        "audio",
    ])
    .expect_err("parse error");

    assert!(err.to_string().contains("unsupported model capability"));
}

#[test]
fn model_selector_errors_keep_subcommand_usage_hint() {
    let err = parse_model_selector("inspect", "REF", "not-a-ref").expect_err("parse error");
    let message = err.to_string();

    assert!(message.contains("model reference must contain only hexadecimal characters"));
    assert!(message.contains("Usage: tentgent model inspect <REF>"));
}

#[test]
fn backend_support_summary_uses_kernel_model_format() {
    assert!(model_format_support_summary(ModelFormat::Gguf).contains("llama-cpp-python"));
    assert!(model_format_support_summary(ModelFormat::Safetensors).contains("transformers"));
    assert!(model_format_support_summary(ModelFormat::Diffusers).contains("diffusers"));
}
