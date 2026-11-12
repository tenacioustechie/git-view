//! Update availability check against published GitHub Releases.
//!
//! Only the "latest" published, non-draft, non-prerelease release is consulted.
//! Nothing is downloaded or installed: the user is sent to the release page.
use crate::contracts::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;

pub const RELEASES_PAGE_PREFIX: &str = "https://github.com/tenacioustechie/git-view/releases/";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/tenacioustechie/git-view/releases/latest";
const MAX_RESPONSE_BYTES: usize = 1_000_000;
const MAX_NOTES_CHARS: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current_version: String,
    /// `None` when no published release exists yet.
    pub latest_version: Option<String>,
    pub available: bool,
    pub release_name: Option<String>,
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    pub notes_truncated: bool,
    pub checked_at: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub body: Option<String>,
}

/// Remembers the release page from the last successful check so the webview can
/// only ask to open that validated URL, never an arbitrary one.
#[derive(Default)]
pub struct Updates {
    release_url: Mutex<Option<String>>,
}

impl Updates {
    pub fn remember(&self, check: &UpdateCheck) {
        *self.release_url.lock().expect("update state poisoned") =
            check.release_url.clone().filter(|_| check.available);
    }
    pub fn release_url(&self) -> AppResult<String> {
        self.release_url
            .lock()
            .expect("update state poisoned")
            .clone()
            .ok_or_else(|| {
                AppError::new(
                    "no_update_available",
                    "Check for updates again before opening the release page.",
                )
            })
    }
}

fn parse_version(value: &str) -> Option<semver::Version> {
    semver::Version::parse(value.strip_prefix('v').unwrap_or(value)).ok()
}

fn valid_release_url(url: &str) -> bool {
    url.starts_with(RELEASES_PAGE_PREFIX)
        && url.len() <= 512
        && !url.contains(['\0', ' ', '\n', '\r', '\\', '"', '<', '>'])
}

fn truncate_notes(body: Option<String>) -> (Option<String>, bool) {
    let Some(body) = body.map(|b| b.trim().to_owned()).filter(|b| !b.is_empty()) else {
        return (None, false);
    };
    if body.chars().count() <= MAX_NOTES_CHARS {
        return (Some(body), false);
    }
    (Some(body.chars().take(MAX_NOTES_CHARS).collect()), true)
}

pub fn no_release(current: &str, checked_at: String) -> UpdateCheck {
    UpdateCheck {
        current_version: current.into(),
        latest_version: None,
        available: false,
        release_name: None,
        release_url: None,
        published_at: None,
        notes: None,
        notes_truncated: false,
        checked_at,
    }
}

/// Compares a published release with the running version. Pure, for testing.
pub fn evaluate(
    current: &str,
    release: GithubRelease,
    checked_at: String,
) -> AppResult<UpdateCheck> {
    let running = parse_version(current).ok_or_else(|| {
        AppError::new(
            "invalid_version",
            format!("Running version {current} is not valid."),
        )
    })?;
    if release.draft || release.prerelease {
        return Ok(no_release(current, checked_at));
    }
    let latest = parse_version(&release.tag_name).ok_or_else(|| {
        AppError::new(
            "invalid_release",
            format!(
                "The latest release tag {:?} is not a version.",
                release.tag_name
            ),
        )
    })?;
    if !valid_release_url(&release.html_url) {
        return Err(AppError::new(
            "invalid_release",
            "The latest release does not link to the Git View releases page.",
        ));
    }
    let (notes, notes_truncated) = truncate_notes(release.body);
    Ok(UpdateCheck {
        current_version: current.into(),
        latest_version: Some(latest.to_string()),
        available: latest > running,
        release_name: release.name.filter(|n| !n.trim().is_empty()),
        release_url: Some(release.html_url),
        published_at: release.published_at,
        notes,
        notes_truncated,
        checked_at,
    })
}

fn network_error(error: reqwest::Error) -> AppError {
    let mut e = AppError::new(
        "update_check_failed",
        if error.is_timeout() {
            "GitHub did not respond in time.".to_owned()
        } else {
            "Could not reach GitHub.".to_owned()
        },
    );
    e.retryable = true;
    e.details = Some(error.to_string());
    e
}

pub async fn check(current: &str) -> AppResult<UpdateCheck> {
    // Only the ring provider is compiled in; a second install attempt is harmless.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent(format!("Git-View/{current}"))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(network_error)?;
    let mut response = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(network_error)?;
    let checked_at = chrono::Utc::now().to_rfc3339();
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(no_release(current, checked_at));
    }
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        let mut e = AppError::new(
            "update_check_rate_limited",
            "GitHub is limiting requests from this network. Try again later.",
        );
        e.retryable = true;
        return Err(e);
    }
    if !status.is_success() {
        let mut e = AppError::new(
            "update_check_failed",
            format!("GitHub returned HTTP {}.", status.as_u16()),
        );
        e.retryable = status.is_server_error();
        return Err(e);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(AppError::new(
                "invalid_release",
                "GitHub's release response was unexpectedly large.",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let release: GithubRelease = serde_json::from_slice(&body).map_err(|e| {
        let mut error = AppError::new(
            "invalid_release",
            "GitHub's release response could not be read.",
        );
        error.details = Some(e.to_string());
        error
    })?;
    evaluate(current, release, checked_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> GithubRelease {
        GithubRelease {
            tag_name: tag.into(),
            name: Some(format!("Git View {tag}")),
            html_url: format!("{RELEASES_PAGE_PREFIX}tag/{tag}"),
            draft: false,
            prerelease: false,
            published_at: Some("2026-11-01T00:00:00Z".into()),
            body: Some("Notes".into()),
        }
    }
    fn at() -> String {
        "2026-11-12T00:00:00Z".into()
    }

    #[test]
    fn newer_release_is_available() {
        let result = evaluate("0.2.1", release("v0.3.0"), at()).unwrap();
        assert!(result.available);
        assert_eq!(result.latest_version.as_deref(), Some("0.3.0"));
        assert_eq!(
            result.release_url.as_deref(),
            Some("https://github.com/tenacioustechie/git-view/releases/tag/v0.3.0")
        );
    }

    #[test]
    fn same_or_older_release_is_not_available() {
        assert!(
            !evaluate("0.2.1", release("v0.2.1"), at())
                .unwrap()
                .available
        );
        assert!(
            !evaluate("0.2.1", release("v0.2.0"), at())
                .unwrap()
                .available
        );
        // Semantic, not lexical, ordering.
        assert!(
            evaluate("0.9.0", release("v0.10.0"), at())
                .unwrap()
                .available
        );
        // A release is newer than its own prerelease.
        assert!(
            evaluate("0.3.0-rc.1", release("v0.3.0"), at())
                .unwrap()
                .available
        );
    }

    #[test]
    fn drafts_and_prereleases_are_ignored() {
        let mut draft = release("v9.0.0");
        draft.draft = true;
        assert_eq!(
            evaluate("0.2.1", draft, at()).unwrap(),
            no_release("0.2.1", at())
        );
        let mut pre = release("v9.0.0-beta.1");
        pre.prerelease = true;
        assert!(!evaluate("0.2.1", pre, at()).unwrap().available);
    }

    #[test]
    fn rejects_bad_tags_and_foreign_urls() {
        assert_eq!(
            evaluate("0.2.1", release("latest"), at()).unwrap_err().code,
            "invalid_release"
        );
        for url in [
            "https://example.com/tenacioustechie/git-view/releases/tag/v1.0.0",
            "https://github.com/someone-else/git-view/releases/tag/v1.0.0",
            "https://github.com/tenacioustechie/git-view/releases/tag/v1 0",
            "http://github.com/tenacioustechie/git-view/releases/tag/v1.0.0",
        ] {
            let mut r = release("v1.0.0");
            r.html_url = url.into();
            assert_eq!(
                evaluate("0.2.1", r, at()).unwrap_err().code,
                "invalid_release",
                "{url}"
            );
        }
    }

    #[test]
    fn parses_github_payload_and_truncates_notes() {
        let long = "é".repeat(MAX_NOTES_CHARS + 5);
        let payload = serde_json::json!({
            "tag_name": "v1.2.3", "name": "", "html_url": format!("{RELEASES_PAGE_PREFIX}tag/v1.2.3"),
            "draft": false, "prerelease": false, "published_at": null, "body": long, "assets": []
        });
        let parsed: GithubRelease = serde_json::from_value(payload).unwrap();
        let result = evaluate("1.0.0", parsed, at()).unwrap();
        assert!(result.available);
        assert_eq!(result.release_name, None);
        assert!(result.notes_truncated);
        assert_eq!(result.notes.unwrap().chars().count(), MAX_NOTES_CHARS);
    }

    #[test]
    fn only_an_available_update_url_can_be_opened() {
        let updates = Updates::default();
        assert!(updates.release_url().is_err());
        let current = evaluate("1.2.3", release("v1.2.3"), at()).unwrap();
        updates.remember(&current);
        assert!(updates.release_url().is_err());
        let newer = evaluate("1.0.0", release("v1.2.3"), at()).unwrap();
        updates.remember(&newer);
        assert_eq!(updates.release_url().unwrap(), newer.release_url.unwrap());
    }

    /// Contacts GitHub; run with `cargo test --lib updates -- --ignored`.
    #[test]
    #[ignore = "requires network access to api.github.com"]
    fn live_latest_release_check() {
        let result = tauri::async_runtime::block_on(check("0.0.1")).unwrap();
        assert!(result.latest_version.is_some(), "{result:?}");
        assert!(result.available);
    }
}
