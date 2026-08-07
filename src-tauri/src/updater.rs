//! Update check against GitHub Releases.
//!
//! Ported from AURA's `apps/desktop/updater.cjs`, which follows the same shape:
//! pure logic lives here behind an injected HTTP client so it can be tested
//! without a network, and `lib.rs` owns the side effects (downloading, staging,
//! spawning the installer, telling the UI).
//!
//! Why not `tauri-plugin-updater`? It needs a signing keypair, a published
//! `latest.json`, and `createUpdaterArtifacts` in the bundle config — none of
//! which exist yet. Reading the GitHub Releases API works with the release
//! pipeline exactly as it stands today. If the signing infrastructure is set up
//! later, the official plugin is the better home for this and the UI here
//! (`UpdateBanner`) can stay as-is.
//!
//! Everything here fails silent by design: a background update check must never
//! interrupt someone who is in the middle of writing music.

use serde::{Deserialize, Serialize};

/// Repository whose releases are checked.
pub const REPO: &str = "janvanwassenhove/PiBeat";

/// User-Agent — the GitHub API rejects requests without one.
const USER_AGENT: &str = "pibeat-desktop";

/// A downloadable file attached to a release.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseAsset {
    pub name: String,
    /// API URL (not `browser_download_url`) — with
    /// `Accept: application/octet-stream` this also works for private repos,
    /// and GitHub answers with a redirect the client follows.
    pub url: String,
    #[serde(default)]
    pub size: u64,
}

/// The subset of GitHub's release payload that matters here.
#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

/// A newer release than the one running.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AvailableUpdate {
    /// Release tag as published, e.g. `v0.3.0`.
    pub tag: String,
    /// Tag without the leading `v`, for display.
    pub version: String,
    /// Release page, opened when there is nothing installable for this platform.
    pub html_url: String,
    /// The one asset this platform can install, if there is one.
    pub asset: Option<ReleaseAsset>,
}

/// Outcome of a check.
///
/// This reports a *status* rather than just "found something / didn't", because
/// the About dialog needs to explain why nothing happened. Without it, a check
/// that silently finds nothing is indistinguishable from a broken one — which
/// is exactly the confusion AURA hit while its repo was still private.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum UpdateStatus {
    /// A newer release exists.
    Update { update: AvailableUpdate },
    /// Already on the latest release (or the latest is a draft/prerelease).
    Current { latest: String },
    /// The repo is private, or the token is missing/invalid. Unauthenticated
    /// calls to a private repo's releases return 404, not 403.
    Unauthorized,
    /// Network failure, rate limit, malformed payload.
    Error { reason: String },
}

/// Minimal HTTP surface the checker needs, so tests can drive it without a
/// network and without a live GitHub.
pub trait HttpClient {
    /// GET returning `(status_code, body)`.
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<(u16, String), String>;
}

/// The real client.
pub struct UreqClient;

impl HttpClient for UreqClient {
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<(u16, String), String> {
        let mut req = ureq::get(url);
        for (name, value) in headers {
            req = req.header(*name, value);
        }
        match req.call() {
            Ok(mut res) => {
                let status = res.status().as_u16();
                let body = res.body_mut().read_to_string().unwrap_or_default();
                Ok((status, body))
            }
            // ureq treats 4xx/5xx as errors; recover the code so the caller can
            // tell "private repo" from "the network is down".
            Err(ureq::Error::StatusCode(code)) => Ok((code, String::new())),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Parse a `MAJOR.MINOR.PATCH` version, tolerating a leading `v` and any
/// suffix (`-beta`, `+build`).
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let s = v.trim().trim_start_matches('v');
    let mut parts = s.split(['.', '-', '+']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    // A trailing suffix is already split off by the separators above.
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Whether `candidate` is a strictly newer version than `current`.
///
/// An unparseable version on either side returns false — refusing to guess is
/// better than nagging someone to "upgrade" to something that might be older.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Pick the one asset this platform can actually install.
///
/// Only Windows has a silent installer here; macOS and Linux artifacts have to
/// be opened by hand, so they deliberately return `None` and the caller falls
/// back to opening the release page.
pub fn pick_asset(assets: &[ReleaseAsset]) -> Option<ReleaseAsset> {
    let wanted: fn(&str) -> bool = if cfg!(target_os = "windows") {
        // tauri-action publishes both an NSIS .exe and an .msi. Prefer the
        // NSIS setup: it takes /S for a silent install, msiexec needs different
        // flags and a different spawn.
        |name: &str| name.ends_with("-setup.exe") || name.ends_with(".exe")
    } else {
        // No silent install path on the other platforms.
        |_name: &str| false
    };
    assets
        .iter()
        .find(|a| wanted(&a.name.to_ascii_lowercase()))
        .cloned()
}

/// Ask GitHub whether a newer release exists.
///
/// `token` is optional and only needed while the repository is private.
pub fn check_for_update(
    client: &dyn HttpClient,
    current_version: &str,
    token: Option<&str>,
) -> UpdateStatus {
    let mut headers: Vec<(&str, String)> = vec![
        ("Accept", "application/vnd.github+json".to_string()),
        ("User-Agent", USER_AGENT.to_string()),
    ];
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        headers.push(("Authorization", format!("Bearer {t}")));
    }

    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let (status, body) = match client.get(&url, &headers) {
        Ok(r) => r,
        Err(reason) => return UpdateStatus::Error { reason },
    };

    // A private repo answers 404 to an unauthenticated caller, so all three of
    // these mean the same thing to a user: "I cannot see the releases".
    if matches!(status, 401 | 403 | 404) {
        return UpdateStatus::Unauthorized;
    }
    if !(200..300).contains(&status) {
        return UpdateStatus::Error {
            reason: format!("HTTP {status}"),
        };
    }

    let release: Release = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return UpdateStatus::Error {
                reason: format!("unreadable release payload: {e}"),
            }
        }
    };

    // Drafts and prereleases are not offered. PiBeat's release workflow
    // publishes as a draft first and un-drafts in a second job, so without this
    // a check landing between the two would offer an update whose assets are
    // still uploading.
    if release.draft || release.prerelease {
        return UpdateStatus::Current {
            latest: current_version.to_string(),
        };
    }

    if !is_newer(&release.tag_name, current_version) {
        return UpdateStatus::Current {
            latest: release.tag_name,
        };
    }

    UpdateStatus::Update {
        update: AvailableUpdate {
            version: release.tag_name.trim_start_matches('v').to_string(),
            tag: release.tag_name,
            html_url: release.html_url,
            asset: pick_asset(&release.assets),
        },
    }
}

/// Download a release asset to `dest`.
///
/// Uses the API asset URL with `Accept: application/octet-stream` so it also
/// works on private repos; GitHub replies with a redirect that ureq follows.
pub fn download_asset(
    asset: &ReleaseAsset,
    token: Option<&str>,
    dest: &std::path::Path,
) -> Result<(), String> {
    let mut req = ureq::get(&asset.url)
        .header("Accept", "application/octet-stream")
        .header("User-Agent", USER_AGENT);
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }

    let mut res = req
        .call()
        .map_err(|e| format!("asset download failed: {e}"))?;

    // Download beside the target and rename on success, so a half-written file
    // from a dropped connection is never mistaken for a staged installer.
    let partial = dest.with_extension("partial");
    {
        let mut reader = res.body_mut().as_reader();
        let mut file = std::fs::File::create(&partial)
            .map_err(|e| format!("cannot create {}: {e}", partial.display()))?;
        std::io::copy(&mut reader, &mut file).map_err(|e| {
            let _ = std::fs::remove_file(&partial);
            format!("download interrupted: {e}")
        })?;
    }
    std::fs::rename(&partial, dest).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        format!("cannot finalise download: {e}")
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Canned HTTP responses, so these tests never touch the network.
    struct FakeClient {
        status: u16,
        body: String,
        seen_headers: RefCell<Vec<(String, String)>>,
    }

    impl FakeClient {
        fn ok(body: &str) -> Self {
            Self {
                status: 200,
                body: body.to_string(),
                seen_headers: RefCell::new(Vec::new()),
            }
        }
        fn status(code: u16) -> Self {
            Self {
                status: code,
                body: String::new(),
                seen_headers: RefCell::new(Vec::new()),
            }
        }
    }

    impl HttpClient for FakeClient {
        fn get(&self, _url: &str, headers: &[(&str, String)]) -> Result<(u16, String), String> {
            *self.seen_headers.borrow_mut() = headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            Ok((self.status, self.body.clone()))
        }
    }

    struct FailingClient;
    impl HttpClient for FailingClient {
        fn get(&self, _url: &str, _headers: &[(&str, String)]) -> Result<(u16, String), String> {
            Err("dns failure".to_string())
        }
    }

    fn release_json(tag: &str, draft: bool, prerelease: bool) -> String {
        format!(
            r#"{{
                "tag_name": "{tag}",
                "html_url": "https://github.com/janvanwassenhove/PiBeat/releases/tag/{tag}",
                "draft": {draft},
                "prerelease": {prerelease},
                "assets": [
                    {{"name": "PiBeat_0.3.0_x64-setup.exe", "url": "https://api.github.com/assets/1", "size": 12}},
                    {{"name": "PiBeat_0.3.0_universal.dmg", "url": "https://api.github.com/assets/2", "size": 34}}
                ]
            }}"#
        )
    }

    #[test]
    fn parses_versions_with_and_without_prefix() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version(" v0.2.1 "), Some((0, 2, 1)));
        assert_eq!(parse_version("2.0.0-beta.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("not-a-version"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn compares_versions_componentwise() {
        assert!(is_newer("v0.3.0", "0.2.1"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.2.2", "0.2.1"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        // 10 > 9 numerically, but "10" < "9" as a string — the mistake this
        // guards against.
        assert!(is_newer("0.10.0", "0.9.0"));
    }

    #[test]
    fn refuses_to_guess_at_unparseable_versions() {
        // Nagging someone to "update" to something that might be older is
        // worse than staying quiet.
        assert!(!is_newer("garbage", "0.2.1"));
        assert!(!is_newer("0.3.0", "garbage"));
    }

    #[test]
    fn reports_an_available_update() {
        let client = FakeClient::ok(&release_json("v0.3.0", false, false));
        match check_for_update(&client, "0.2.1", None) {
            UpdateStatus::Update { update } => {
                assert_eq!(update.tag, "v0.3.0");
                assert_eq!(update.version, "0.3.0");
                assert!(update.html_url.contains("v0.3.0"));
            }
            other => panic!("expected an update, got {other:?}"),
        }
    }

    #[test]
    fn reports_current_when_up_to_date() {
        let client = FakeClient::ok(&release_json("v0.2.1", false, false));
        assert_eq!(
            check_for_update(&client, "0.2.1", None),
            UpdateStatus::Current {
                latest: "v0.2.1".to_string()
            }
        );
    }

    #[test]
    fn ignores_drafts_and_prereleases() {
        // PiBeat's release workflow publishes a draft and un-drafts it in a
        // second job; a check landing between the two must not offer an update
        // whose assets are still uploading.
        let draft = FakeClient::ok(&release_json("v0.9.0", true, false));
        assert!(matches!(
            check_for_update(&draft, "0.2.1", None),
            UpdateStatus::Current { .. }
        ));

        let pre = FakeClient::ok(&release_json("v0.9.0", false, true));
        assert!(matches!(
            check_for_update(&pre, "0.2.1", None),
            UpdateStatus::Current { .. }
        ));
    }

    #[test]
    fn treats_404_401_403_as_unauthorized() {
        // A private repo answers 404 to an anonymous caller — all three mean
        // "I cannot see the releases" to a user.
        for code in [401, 403, 404] {
            assert_eq!(
                check_for_update(&FakeClient::status(code), "0.2.1", None),
                UpdateStatus::Unauthorized,
                "HTTP {code} should read as unauthorized"
            );
        }
    }

    #[test]
    fn surfaces_network_and_server_failures_as_error() {
        assert!(matches!(
            check_for_update(&FailingClient, "0.2.1", None),
            UpdateStatus::Error { .. }
        ));
        assert!(matches!(
            check_for_update(&FakeClient::status(500), "0.2.1", None),
            UpdateStatus::Error { .. }
        ));
        // Malformed JSON must not panic.
        assert!(matches!(
            check_for_update(&FakeClient::ok("{ not json"), "0.2.1", None),
            UpdateStatus::Error { .. }
        ));
    }

    #[test]
    fn sends_the_token_only_when_there_is_one() {
        let client = FakeClient::ok(&release_json("v0.2.1", false, false));
        check_for_update(&client, "0.2.1", Some("ghp_secret"));
        assert!(client
            .seen_headers
            .borrow()
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer ghp_secret"));

        let anon = FakeClient::ok(&release_json("v0.2.1", false, false));
        check_for_update(&anon, "0.2.1", None);
        assert!(!anon.seen_headers.borrow().iter().any(|(k, _)| k == "Authorization"));

        // An empty token is the same as no token — sending `Bearer ` gets a 401.
        let empty = FakeClient::ok(&release_json("v0.2.1", false, false));
        check_for_update(&empty, "0.2.1", Some(""));
        assert!(!empty.seen_headers.borrow().iter().any(|(k, _)| k == "Authorization"));
    }

    #[test]
    fn always_sends_a_user_agent() {
        // The GitHub API rejects requests without one.
        let client = FakeClient::ok(&release_json("v0.2.1", false, false));
        check_for_update(&client, "0.2.1", None);
        assert!(client
            .seen_headers
            .borrow()
            .iter()
            .any(|(k, _)| k == "User-Agent"));
    }

    #[test]
    fn picks_the_installable_asset_for_this_platform() {
        let assets = vec![
            ReleaseAsset {
                name: "PiBeat_0.3.0_universal.dmg".into(),
                url: "u1".into(),
                size: 0,
            },
            ReleaseAsset {
                name: "PiBeat_0.3.0_x64-setup.exe".into(),
                url: "u2".into(),
                size: 0,
            },
            ReleaseAsset {
                name: "pibeat_0.3.0_amd64.AppImage".into(),
                url: "u3".into(),
                size: 0,
            },
        ];
        let picked = pick_asset(&assets);
        if cfg!(target_os = "windows") {
            assert_eq!(picked.map(|a| a.name), Some("PiBeat_0.3.0_x64-setup.exe".into()));
        } else {
            // No silent install path off Windows — the caller opens the
            // release page instead of staging something it cannot run.
            assert_eq!(picked, None);
        }
    }

    #[test]
    fn handles_a_release_with_no_assets() {
        let body = r#"{"tag_name":"v0.3.0","html_url":"https://example.invalid","draft":false,"prerelease":false,"assets":[]}"#;
        match check_for_update(&FakeClient::ok(body), "0.2.1", None) {
            UpdateStatus::Update { update } => assert!(update.asset.is_none()),
            other => panic!("expected an update with no asset, got {other:?}"),
        }
    }
}
