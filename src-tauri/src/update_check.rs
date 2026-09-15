use serde::{Deserialize, Serialize};

const RELEASES_URL: &str =
    "https://api.github.com/repos/KUDABEZHATb/backup-verify/releases/latest";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
}

/// Compares two `major.minor.patch` version strings (a leading `v` in
/// `latest`, as GitHub tags use, is stripped). Anything that doesn't parse
/// as three numeric parts is treated as not newer, since a malformed tag
/// isn't grounds to nag the user about an update.
fn is_newer(latest: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let v = v.trim_start_matches('v');
        let mut parts = v.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        Some((major, minor, patch))
    }
    match (parse(latest), parse(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    use tauri_plugin_http::reqwest;

    let client = reqwest::ClientBuilder::new()
        .user_agent(format!("backup-verify/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(RELEASES_URL)
        .send()
        .await
        .map_err(|_| "Не удалось связаться с GitHub — проверьте подключение к интернету.".to_string())?;

    if !response.status().is_success() {
        return Err(format!("GitHub вернул ошибку: {}", response.status()));
    }

    let body = response.text().await.map_err(|e| e.to_string())?;
    let release: GithubRelease = serde_json::from_str(&body).map_err(|e| e.to_string())?;

    let current_version = app.package_info().version.to_string();
    let latest_version = release.tag_name.trim_start_matches('v').to_string();

    Ok(UpdateInfo {
        update_available: is_newer(&latest_version, &current_version),
        current_version,
        latest_version,
        release_url: release.html_url,
    })
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn detects_newer_patch() {
        assert!(is_newer("0.1.1", "0.1.0"));
    }

    #[test]
    fn detects_newer_with_v_prefix() {
        assert!(is_newer("v0.2.0", "0.1.0"));
    }

    #[test]
    fn same_version_is_not_newer() {
        assert!(!is_newer("0.1.0", "0.1.0"));
    }

    #[test]
    fn older_is_not_newer() {
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn malformed_tag_is_not_newer() {
        assert!(!is_newer("not-a-version", "0.1.0"));
    }
}
