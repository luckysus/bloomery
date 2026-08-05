use crate::storage::secrets::SecretValue;
use reqwest::header::HeaderMap;
use reqwest::Url;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

const REDACTED: &str = "[REDACTED]";

#[derive(Default)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_secret(mut self, secret: &SecretValue) -> Self {
        let secret = secret.expose().to_string();
        if !self.secrets.iter().any(|known| known == &secret) {
            self.secrets.push(secret);
            self.secrets
                .sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
        }
        self
    }

    pub fn redact_headers(&self, headers: &HeaderMap) -> BTreeMap<String, String> {
        headers
            .iter()
            .map(|(name, value)| {
                let name = name.as_str().to_ascii_lowercase();
                let value = if sensitive_header(&name) {
                    REDACTED.to_string()
                } else {
                    value
                        .to_str()
                        .map(|value| self.redact_text(value))
                        .unwrap_or_else(|_| REDACTED.to_string())
                };
                (name, value)
            })
            .collect()
    }

    pub fn redact_url(&self, url: &Url) -> String {
        self.redact_url_inner(url, true)
    }

    fn redact_url_inner(&self, url: &Url, redact_nested_urls: bool) -> String {
        let mut redacted = url.clone();
        if !url.username().is_empty() && redacted.set_username(REDACTED).is_err() {
            return REDACTED.to_string();
        }
        if url.password().is_some_and(|value| !value.is_empty())
            && redacted.set_password(Some(REDACTED)).is_err()
        {
            return REDACTED.to_string();
        }
        if url.fragment().is_some() {
            redacted.set_fragment(Some(REDACTED));
        }
        if url.query().is_some() {
            let pairs = url
                .query_pairs()
                .map(|(key, value)| {
                    let value = if sensitive_key(&key) {
                        REDACTED.to_string()
                    } else {
                        let value = self.redact_known_secrets(&value);
                        if redact_nested_urls {
                            self.redact_embedded_urls_inner(&value, false)
                        } else {
                            value
                        }
                    };
                    (key.into_owned(), value)
                })
                .collect::<Vec<_>>();
            redacted.set_query(None);
            let mut query = redacted.query_pairs_mut();
            for (key, value) in pairs {
                query.append_pair(&key, &value);
            }
        }
        redacted.to_string()
    }

    pub fn redact_json(&self, value: &Value) -> Value {
        match value {
            Value::Object(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| {
                        let value = if sensitive_key(key) {
                            Value::String(REDACTED.to_string())
                        } else {
                            self.redact_json(value)
                        };
                        (key.clone(), value)
                    })
                    .collect(),
            ),
            Value::Array(values) => {
                Value::Array(values.iter().map(|value| self.redact_json(value)).collect())
            }
            Value::String(value) => Value::String(self.redact_text(value)),
            _ => value.clone(),
        }
    }

    pub fn redact_body(&self, body: &str) -> String {
        serde_json::from_str::<Value>(body)
            .map(|value| self.redact_json(&value).to_string())
            .unwrap_or_else(|_| self.redact_text(body))
    }

    pub fn redact_text(&self, value: &str) -> String {
        let value = self.redact_known_secrets(value);
        self.redact_embedded_urls(&value)
    }

    fn redact_known_secrets(&self, value: &str) -> String {
        self.secrets.iter().fold(value.to_string(), |text, secret| {
            text.replace(secret, REDACTED)
        })
    }

    fn redact_embedded_urls(&self, value: &str) -> String {
        self.redact_embedded_urls_inner(value, true)
    }

    fn redact_embedded_urls_inner(&self, value: &str, redact_nested_urls: bool) -> String {
        let mut output = String::with_capacity(value.len());
        let mut rest = value;
        while let Some(start) = embedded_url_start(rest) {
            output.push_str(&rest[..start]);
            let candidate = &rest[start..];
            let end = embedded_url_end(candidate);
            let token = &candidate[..end];
            let url_text = token.trim_end_matches(|character: char| {
                matches!(character, ',' | '.' | ';' | '!' | ')' | ']' | '}')
            });
            match Url::parse(url_text) {
                Ok(url) => output.push_str(&self.redact_url_inner(&url, redact_nested_urls)),
                Err(_) => output.push_str(url_text),
            }
            output.push_str(&token[url_text.len()..]);
            rest = &candidate[end..];
        }
        output.push_str(rest);
        output
    }
}

impl fmt::Debug for Redactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Redactor")
            .field("secret_count", &self.secrets.len())
            .finish()
    }
}

fn sensitive_header(name: &str) -> bool {
    matches!(
        name,
        "authorization" | "proxy-authorization" | "x-api-key" | "api-key" | "cookie" | "set-cookie"
    )
}

fn sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "api_key"
            | "apikey"
            | "api-key"
            | "key"
            | "token"
            | "access_token"
            | "refresh_token"
            | "authorization"
            | "password"
            | "secret"
            | "secret_value"
            | "signature"
            | "x-amz-credential"
            | "x-amz-security-token"
            | "x-amz-signature"
    )
}

fn embedded_url_start(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    (0..bytes.len()).find(|start| {
        bytes
            .get(*start..start + 7)
            .is_some_and(|value| value.eq_ignore_ascii_case(b"http://"))
            || bytes
                .get(*start..start + 8)
                .is_some_and(|value| value.eq_ignore_ascii_case(b"https://"))
    })
}

fn embedded_url_end(value: &str) -> usize {
    let scheme_length = if value
        .as_bytes()
        .get(..8)
        .is_some_and(|value| value.eq_ignore_ascii_case(b"https://"))
    {
        8
    } else {
        7
    };
    let tail = &value[scheme_length..];
    let next_url = adjacent_url_start(tail).map(|index| scheme_length + index);
    let terminator = tail.char_indices().find_map(|(index, character)| {
        is_url_terminator(character).then_some(scheme_length + index)
    });
    next_url
        .into_iter()
        .chain(terminator)
        .min()
        .unwrap_or(value.len())
}

fn adjacent_url_start(value: &str) -> Option<usize> {
    let mut search_from = 0;
    while search_from < value.len() {
        let index = search_from + embedded_url_start(&value[search_from..])?;
        if value[..index]
            .chars()
            .next_back()
            .is_some_and(is_adjacent_url_delimiter)
        {
            return Some(index);
        }
        search_from = index
            + if value
                .as_bytes()
                .get(index..index + 8)
                .is_some_and(|value| value.eq_ignore_ascii_case(b"https://"))
            {
                8
            } else {
                7
            };
    }
    None
}

fn is_adjacent_url_delimiter(character: char) -> bool {
    matches!(
        character,
        ',' | '.' | ';' | '!' | ')' | ']' | '}' | '，' | '。' | '；' | '！' | '？' | '、' | '：'
    )
}

fn is_url_terminator(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\'' | '<' | '>' | '，' | '。' | '；' | '！' | '？' | '、' | '：'
        )
}
