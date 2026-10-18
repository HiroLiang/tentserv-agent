//! Bounded diagnostic summaries for persisted and displayed model proofs.
//!
//! This is a credential-pattern boundary, not a general PII detector. It never
//! reads environment variables or credentials to discover their secret values.

const REDACTED: &str = "[redacted]";
const MAX_PROOF_ERROR_CHARS: usize = 500;

pub(crate) fn sanitize_proof_error(error: String) -> String {
    let mut redacted = String::with_capacity(error.len());
    let mut cursor = 0;
    while cursor < error.len() {
        let remaining = &error[cursor..];
        let first = remaining.as_bytes()[0];
        if !key_byte(first) {
            let character = remaining.chars().next().expect("nonempty remainder");
            redacted.push(character);
            cursor += character.len_utf8();
            continue;
        }

        let end = cursor + remaining.bytes().take_while(|byte| key_byte(*byte)).count();
        let key = &error[cursor..end];
        let normalized = key
            .bytes()
            .filter(|byte| byte.is_ascii_alphanumeric())
            .map(|byte| char::from(byte.to_ascii_lowercase()))
            .collect::<String>();
        let env_name = is_secret_env_name(key);
        redacted.push_str(if env_name { "[redacted-env]" } else { key });
        cursor = end;

        let authorization = matches!(normalized.as_str(), "authorization" | "proxyauthorization");
        let bearer = normalized == "bearer";
        if !env_name && !authorization && !bearer && !sensitive_key(&normalized) {
            continue;
        }

        let mut value_start = end;
        // JSON/Python dictionary keys can have a closing quote before ':' or '='.
        if matches!(error.as_bytes().get(value_start), Some(b'\'' | b'"')) {
            value_start += 1;
        } else if error.as_bytes().get(value_start) == Some(&b'\\')
            && matches!(error.as_bytes().get(value_start + 1), Some(b'\'' | b'"'))
        {
            value_start += 2;
        }
        value_start = skip_whitespace(&error, value_start);
        let assignment = matches!(error.as_bytes().get(value_start), Some(b':' | b'='));
        if assignment {
            value_start = skip_whitespace(&error, value_start + 1);
        } else if !(bearer || authorization) || value_start == end {
            continue;
        }
        if value_start == error.len() {
            continue;
        }

        let value_end = credential_end(&error, value_start);
        if value_end == value_start {
            continue;
        }
        redacted.push_str(&error[end..value_start]);
        let quote = error.as_bytes()[value_start];
        if matches!(quote, b'\'' | b'"') {
            redacted.push(char::from(quote));
            redacted.push_str(REDACTED);
            // Also close an unterminated quoted secret, never expose its tail.
            redacted.push(char::from(quote));
        } else {
            redacted.push_str(REDACTED);
        }
        cursor = value_end;
    }

    let compact = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = compact.chars();
    let mut bounded = characters
        .by_ref()
        .take(MAX_PROOF_ERROR_CHARS)
        .collect::<String>();
    if characters.next().is_some() {
        bounded.push_str("...");
    }
    bounded
}

fn key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn is_secret_env_name(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    matches!(upper.as_str(), "HF_TOKEN" | "HUGGING_FACE_HUB_TOKEN")
        || upper.ends_with("_API_KEY")
        || upper.ends_with("_ACCESS_TOKEN")
        || upper.ends_with("_SECRET_KEY")
        || upper.ends_with("_TOKEN")
        || upper.ends_with("_SECRET")
        || upper.ends_with("_PASSWORD")
}

fn sensitive_key(normalized: &str) -> bool {
    matches!(
        normalized,
        "key"
            | "apikey"
            | "xapikey"
            | "token"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "authtoken"
            | "password"
            | "passwd"
            | "secret"
            | "clientsecret"
            | "secretkey"
    ) || normalized.ends_with("apitoken")
        || normalized.ends_with("apikey")
        || normalized.ends_with("accesstoken")
        || normalized.ends_with("token")
}

fn skip_whitespace(text: &str, start: usize) -> usize {
    start
        + text[start..]
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>()
}

fn credential_end(text: &str, start: usize) -> usize {
    let value = &text[start..];
    if value.starts_with(REDACTED) {
        return start + REDACTED.len();
    }
    let quote = value.as_bytes()[0];
    if matches!(quote, b'\'' | b'"') {
        let mut escaped = false;
        for (index, byte) in value.bytes().enumerate().skip(1) {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                return start + index + 1;
            }
        }
        return text.len();
    }
    // Unquoted diagnostics do not reliably delimit a value at spaces or quotes
    // (e.g. Authorization: Bearer "value"). Redact the whole field conservatively.
    start
        + value
            .bytes()
            .take_while(|byte| !matches!(byte, b'\r' | b'\n' | b',' | b';' | b'&' | b'}' | b']'))
            .count()
}

#[cfg(test)]
mod tests {
    use super::sanitize_proof_error;

    #[test]
    fn proof_error_redacts_synthetic_credential_values() {
        for input in [
            "HF_TOKEN=synthetic-secret",
            "OPENAI_API_KEY = synthetic-secret",
            "HUGGING_FACE_HUB_TOKEN: synthetic-secret",
            "TENTGENT_DAEMON_TOKEN=synthetic-secret",
            "ANTHROPIC_API_KEY=synthetic-secret",
            "GEMINI_API_KEY=synthetic-secret",
            "GOOGLE_API_KEY=synthetic-secret",
            "{\"api_key\":\"synthetic-secret\"}",
            "{'access_token': 'synthetic-secret'}",
            "x-api-key: synthetic-secret",
            "apiKey=synthetic-secret",
            "refresh_token=synthetic-secret",
            "client_secret=synthetic-secret",
            "Authorization: Bearer synthetic-secret",
            "Authorization: Bearer \"synthetic-secret\"",
            "authorization=Basic synthetic-secret",
            "{\"Authorization\": \"Bearer synthetic-secret\"}",
            "Bearer synthetic-secret",
            "https://example.invalid/failure?token=synthetic-secret&status=500",
            "https://example.invalid/failure?key=synthetic-secret&status=500",
            "api_key=\"synthetic-secret with spaces\"; status=500",
            "api_key=\"synthetic-secret\\\"tail\"; status=500",
            "api_key=\"synthetic-secret without closing quote",
            r#"{\"api_key\": \"synthetic-secret\"}"#,
            "token=synthetic-secret with spaces; status=500",
        ] {
            let sanitized = sanitize_proof_error(format!("load failed: {input}"));
            assert!(!sanitized.contains("synthetic-secret"), "{sanitized}");
            assert!(sanitized.contains("[redacted]"), "{sanitized}");
            assert!(sanitized.starts_with("load failed:"));
            assert_eq!(sanitize_proof_error(sanitized.clone()), sanitized);
        }
    }

    #[test]
    fn proof_error_preserves_ordinary_diagnostics_and_legacy_marker_masking() {
        assert_eq!(
            sanitize_proof_error("  model load failed\n CUDA unavailable  ".into()),
            "model load failed CUDA unavailable"
        );
        assert_eq!(
            sanitize_proof_error("OPENAI_API_KEY is not configured".into()),
            "[redacted-env] is not configured"
        );
        assert_eq!(
            sanitize_proof_error("token count 42; tokenizer unavailable".into()),
            "token count 42; tokenizer unavailable"
        );
    }

    #[test]
    fn proof_error_redacts_before_unicode_safe_truncation() {
        let sanitized =
            sanitize_proof_error(format!("{} api_key=synthetic-secret", "錯".repeat(495)));
        assert_eq!(sanitized.chars().count(), 503);
        assert!(!sanitized.contains("synthetic-secret"));
        assert!(sanitized.ends_with("..."));
    }
}
