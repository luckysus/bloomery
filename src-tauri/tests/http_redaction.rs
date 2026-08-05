use bloomery::diagnostics::redaction::Redactor;
use bloomery::providers::http::{ProviderError, ProviderErrorCode};
use bloomery::storage::secrets::{SecretError, SecretRef, SecretStore, SecretValue};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{StatusCode, Url};

struct SingleSecretStore(SecretValue);

impl SecretStore for SingleSecretStore {
    fn set(&self, _reference: &SecretRef, _value: &SecretValue) -> Result<(), SecretError> {
        Err(SecretError::backend("read-only test store"))
    }

    fn get(&self, _reference: &SecretRef) -> Result<SecretValue, SecretError> {
        Ok(self.0.clone())
    }

    fn delete(&self, _reference: &SecretRef) -> Result<(), SecretError> {
        Err(SecretError::backend("read-only test store"))
    }
}

fn redactor() -> Redactor {
    let store = SingleSecretStore(SecretValue::new("sk-known-secret").unwrap());
    let reference = SecretRef::new(uuid::Uuid::new_v4(), "api_key").unwrap();
    let secret = store.get(&reference).unwrap();
    Redactor::new().with_secret(&secret)
}

#[test]
fn authorization_and_api_key_headers_are_redacted() {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer sk-known-secret"),
    );
    headers.insert("x-api-key", HeaderValue::from_static("another-secret"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));

    let redacted = redactor().redact_headers(&headers);

    assert_eq!(redacted["authorization"], "[REDACTED]");
    assert_eq!(redacted["x-api-key"], "[REDACTED]");
    assert_eq!(redacted["content-type"], "application/json");
    assert!(!format!("{redacted:?}").contains("sk-known-secret"));
}

#[test]
fn sensitive_url_query_values_are_redacted_without_changing_other_values() {
    let url = Url::parse(
        "https://provider.example/v1/models?api_key=sk-known-secret&query=Q355B&token=abc",
    )
    .unwrap();

    let redacted = Url::parse(&redactor().redact_url(&url)).unwrap();
    let query = redacted
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(
        query.get("api_key").map(|value| value.as_ref()),
        Some("[REDACTED]")
    );
    assert_eq!(
        query.get("token").map(|value| value.as_ref()),
        Some("[REDACTED]")
    );
    assert_eq!(
        query.get("query").map(|value| value.as_ref()),
        Some("Q355B")
    );
}

#[test]
fn url_userinfo_and_fragment_credentials_are_redacted() {
    let url = Url::parse(
        "https://url-user:url-password@provider.example/v1/file?query=Q355B#access_token=fragment-secret",
    )
    .unwrap();

    let redacted_text = redactor().redact_url(&url);
    let redacted = Url::parse(&redacted_text).unwrap();

    for secret in ["url-user", "url-password", "fragment-secret"] {
        assert!(!redacted_text.contains(secret), "leaked {secret}");
    }
    assert_eq!(redacted.host_str(), Some("provider.example"));
    assert_eq!(redacted.path(), "/v1/file");
    assert_eq!(redacted.query(), Some("query=Q355B"));
    assert!(!redacted.username().is_empty());
    assert!(redacted.password().is_some());
    assert!(redacted.fragment().is_some());
}

#[test]
fn body_urls_redact_userinfo_and_fragments() {
    let url =
        "https://body-user:body-password@provider.example/v1/file?query=Q355B#token=body-fragment";
    let body = serde_json::json!({"message": url}).to_string();

    let redacted_body = redactor().redact_body(&body);
    let redacted_json: serde_json::Value = serde_json::from_str(&redacted_body).unwrap();
    let redacted_url = Url::parse(redacted_json["message"].as_str().unwrap()).unwrap();

    for secret in ["body-user", "body-password", "body-fragment"] {
        assert!(!redacted_body.contains(secret), "leaked {secret}");
    }
    assert_eq!(redacted_url.host_str(), Some("provider.example"));
    assert_eq!(redacted_url.path(), "/v1/file");
    assert_eq!(redacted_url.query(), Some("query=Q355B"));
}

#[test]
fn signed_urls_in_json_strings_are_redacted_from_provider_errors() {
    let signed_url = concat!(
        "https://storage.example/artifact.zip?token=token-value&api_key=key-value",
        "&signature=sig-value&X-Amz-Algorithm=AWS4-HMAC-SHA256",
        "&X-Amz-Credential=AKIAEXAMPLE%2F20260731%2Fcn-north-1%2Fs3%2Faws4_request",
        "&X-Amz-Security-Token=session-value&X-Amz-Signature=aws-signature",
        "&response-content-type=application%2Fzip"
    );
    let body = serde_json::json!({"message": signed_url}).to_string();

    let redacted_body = redactor().redact_body(&body);
    let redacted_json: serde_json::Value = serde_json::from_str(&redacted_body).unwrap();
    let redacted_url = Url::parse(redacted_json["message"].as_str().unwrap()).unwrap();
    let query = redacted_url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();

    for key in [
        "token",
        "api_key",
        "signature",
        "X-Amz-Credential",
        "X-Amz-Security-Token",
        "X-Amz-Signature",
    ] {
        assert_eq!(
            query.get(key).map(|value| value.as_ref()),
            Some("[REDACTED]"),
            "query key {key}"
        );
    }
    assert_eq!(
        query.get("X-Amz-Algorithm").map(|value| value.as_ref()),
        Some("AWS4-HMAC-SHA256")
    );
    assert_eq!(
        query
            .get("response-content-type")
            .map(|value| value.as_ref()),
        Some("application/zip")
    );

    let display =
        ProviderError::from_status(StatusCode::BAD_GATEWAY, &body, &redactor()).to_string();
    for secret in [
        "token-value",
        "key-value",
        "sig-value",
        "AKIAEXAMPLE",
        "session-value",
        "aws-signature",
    ] {
        assert!(!display.contains(secret), "leaked {secret}");
    }
    assert!(display.contains("artifact.zip"));
    assert!(display.contains("AWS4-HMAC-SHA256"));
}

#[test]
fn signed_urls_in_plain_text_are_redacted_without_hiding_harmless_values() {
    let body = concat!(
        "download: https://storage.example/full.zip?token=plain-token",
        "&X-Amz-Signature=plain-signature&query=Q355B, then retry"
    );

    let redacted = redactor().redact_body(body);

    assert!(redacted.starts_with("download: https://storage.example/full.zip?"));
    assert!(redacted.ends_with(", then retry"));
    assert!(redacted.contains("query=Q355B"));
    assert!(!redacted.contains("plain-token"));
    assert!(!redacted.contains("plain-signature"));
}

#[test]
fn raw_nested_url_query_is_redacted_without_splitting_the_outer_url() {
    let body = "https://a.example/?next=https://b.example/path&token=nested-secret";

    let redacted = redactor().redact_body(body);
    let outer = Url::parse(&redacted).unwrap();
    let query = outer
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();

    assert!(!redacted.contains("nested-secret"));
    assert!(redacted.contains("a.example"));
    assert!(redacted.contains("b.example"));
    assert_eq!(outer.host_str(), Some("a.example"));
    assert_eq!(
        query.get("next").map(|value| value.as_ref()),
        Some("https://b.example/path")
    );
    assert_eq!(
        query.get("token").map(|value| value.as_ref()),
        Some("[REDACTED]")
    );
}

#[test]
fn encoded_nested_url_query_value_is_redacted() {
    let body = concat!(
        "https://a.example/?next=https%3A%2F%2Fb.example%2Fpath%3Ftoken%3Dencoded-secret",
        "&query=Q355B"
    );

    let redacted = redactor().redact_body(body);
    let outer = Url::parse(&redacted).unwrap();
    let query = outer
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    let nested = Url::parse(query.get("next").unwrap()).unwrap();
    let nested_query = nested
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();

    assert!(!redacted.contains("encoded-secret"));
    assert_eq!(outer.host_str(), Some("a.example"));
    assert_eq!(nested.host_str(), Some("b.example"));
    assert_eq!(nested.path(), "/path");
    assert_eq!(
        nested_query.get("token").map(|value| value.as_ref()),
        Some("[REDACTED]")
    );
    assert_eq!(
        query.get("query").map(|value| value.as_ref()),
        Some("Q355B")
    );
}

#[test]
fn adjacent_signed_urls_preserve_ascii_and_unicode_prose_delimiters() {
    let first = "https://first.example/one?query=A&token=first-secret";
    let second = "https://second.example/two?query=B&signature=second-secret";

    for delimiter in [",", "，", "。"] {
        let body = format!("前文{first}{delimiter}{second}。后文");

        let redacted = redactor().redact_body(&body);

        assert!(redacted.starts_with("前文https://first.example/one?"));
        assert!(redacted.contains("https://second.example/two?"));
        assert!(redacted.contains(delimiter));
        assert!(redacted.ends_with("。后文"));
        assert!(redacted.contains("query=A"));
        assert!(redacted.contains("query=B"));
        assert!(!redacted.contains("first-secret"));
        assert!(!redacted.contains("second-secret"));
    }
}

#[test]
fn json_keys_and_known_values_are_redacted_recursively() {
    let body = serde_json::json!({
        "error": {
            "api_key": "different-secret",
            "message": "upstream rejected sk-known-secret",
            "details": [{"access_token": "token-value"}]
        }
    });

    let redacted = redactor().redact_json(&body);
    let text = redacted.to_string();

    assert_eq!(redacted["error"]["api_key"], "[REDACTED]");
    assert_eq!(
        redacted["error"]["details"][0]["access_token"],
        "[REDACTED]"
    );
    assert!(redacted["error"]["message"]
        .as_str()
        .unwrap()
        .contains("[REDACTED]"));
    for secret in ["different-secret", "sk-known-secret", "token-value"] {
        assert!(!text.contains(secret));
    }
}

#[test]
fn provider_errors_use_stable_categories_and_redacted_bodies() {
    let authentication = ProviderError::from_status(
        StatusCode::UNAUTHORIZED,
        r#"{"api_key":"sk-known-secret","message":"bad key"}"#,
        &redactor(),
    );
    let quota = ProviderError::from_status(
        StatusCode::TOO_MANY_REQUESTS,
        "quota exceeded for sk-known-secret",
        &redactor(),
    );
    let upstream =
        ProviderError::from_status(StatusCode::BAD_GATEWAY, "provider failed", &redactor());

    assert_eq!(authentication.code(), ProviderErrorCode::Authentication);
    assert_eq!(quota.code(), ProviderErrorCode::Quota);
    assert_eq!(upstream.code(), ProviderErrorCode::ProviderResponse);
    assert!(!authentication.to_string().contains("sk-known-secret"));
    assert!(!quota.to_string().contains("sk-known-secret"));
}
