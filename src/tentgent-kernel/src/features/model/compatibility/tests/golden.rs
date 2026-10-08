use super::super::*;

// Expected JSON is a schema fixture, not serialized from the type under test.
// SHA-256 independently checked with Python hashlib over these exact UTF-8 bytes.
const ADAPTER_EXECUTION_JSON: &str = r#"{"identity_version":1,"components":{"model_ref":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","capability":"chat","primary_format":"mlx","quantization":{"kind":"quantized","method":"mlx","variant":"affine","bits":{"kind":"mixed"},"group_size":{"kind":"fixed","size":64}},"backend":"mlx","runtime_family":"mlx-lm","runtime":{"package":"mlx-lm","version":"0.30.2"},"profile":{"kind":"selected","id":"chat/default","version":1},"platform":{"os":"macos","architecture":"aarch64"},"device_class":"metal","adapter":{"kind":"selected","adapter_ref":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","load_identity":"LoRA/v1"},"observation":{"kind":"execution","input":{"family":"chat","modalities":["text"],"provider":"openai","attributes":{"tool_calls":true,"structured_output":true}},"output":{"family":"chat","modalities":["text"],"streaming":true,"format":"json"}}}}"#;
const ADAPTER_EXECUTION_SHA256: &str =
    "5fec7b3af3dfe654b59d22e2e5afa95eaa5ba7fe98cb64161ae20d416de2894b";

#[test]
fn adapter_quantized_execution_golden_vector_freezes_all_nested_ordering() {
    let tuple: CompatibilityTuple = serde_json::from_str(ADAPTER_EXECUTION_JSON).unwrap();
    assert_eq!(tuple.canonical_json().unwrap(), ADAPTER_EXECUTION_JSON);
    assert_eq!(
        tuple.key().unwrap().tuple_sha256(),
        ADAPTER_EXECUTION_SHA256
    );
    let toml = toml::to_string(&tuple).unwrap();
    let reread: CompatibilityTuple = toml::from_str(&toml).unwrap();
    assert_eq!(reread.canonical_json().unwrap(), ADAPTER_EXECUTION_JSON);
    assert_eq!(
        reread.key().unwrap().tuple_sha256(),
        ADAPTER_EXECUTION_SHA256
    );
}
