use crate::providers::http::{build_client, HttpClientConfig};
use chrono::Utc;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

const RELEASE_API: &str = "https://api.github.com/repos/luckysus/Suna/releases/latest";

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    html_url: String,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
}

fn version_tuple(value: &str) -> Option<(u64, u64, u64)> {
    let trimmed = value.trim().trim_start_matches(['v', 'V']);
    let mut parts = trimmed.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.split(['-', '+']).next()?.parse().ok()?;
    Some((major, minor, patch))
}

#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateCheckResult, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let checked_at = Utc::now().to_rfc3339();
    let client = build_client(&HttpClientConfig {
        request_timeout: std::time::Duration::from_secs(10),
        ..HttpClientConfig::default()
    })
    .map_err(|error| format!("create update client failed: {error}"))?;
    let response = client
        .get(RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("check updates failed: {error}"))?;
    if response.status() != StatusCode::OK {
        return Err(format!("update service returned {}", response.status()));
    }
    let release = response
        .json::<ReleaseResponse>()
        .await
        .map_err(|error| format!("decode update response failed: {error}"))?;
    if release.draft || release.prerelease {
        return Ok(UpdateCheckResult {
            current_version,
            latest_version: None,
            update_available: false,
            release_url: None,
            published_at: None,
            checked_at,
        });
    }
    let latest_version = release
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    let update_available = match (
        version_tuple(&current_version),
        version_tuple(&latest_version),
    ) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    };
    Ok(UpdateCheckResult {
        current_version,
        latest_version: Some(latest_version),
        update_available,
        release_url: Some(release.html_url),
        published_at: release.published_at,
        checked_at,
    })
}

#[cfg(test)]
mod tests {
    use super::version_tuple;

    #[test]
    fn parses_release_versions_with_tags_and_metadata() {
        assert_eq!(version_tuple("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(version_tuple("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(version_tuple("1.2"), None);
    }
}
