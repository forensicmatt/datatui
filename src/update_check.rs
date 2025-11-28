//! Update check functionality for checking GitHub releases

use color_eyre::Result;
use chrono::{DateTime, Utc, Duration};
use serde::Deserialize;

const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/forensicmatt/datatui/releases/latest";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    published_at: String,
}

/// Check if an update is available by comparing current version with latest GitHub release (async)
pub async fn check_for_update_async(current_version: &str) -> Result<Option<UpdateInfo>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("datatui-update-checker")
        .build()?;

    let response = client.get(GITHUB_RELEASES_API).send().await?;
    
    if !response.status().is_success() {
        return Ok(None);
    }

    let release: GitHubRelease = response.json().await?;
    
    // Extract version from tag_name (format: v0.3.0)
    let latest_version = release.tag_name.trim_start_matches('v');
    let current_version_clean = current_version.trim_start_matches('v');

    if is_newer_version(latest_version, current_version_clean) {
        Ok(Some(UpdateInfo {
            latest_version: release.tag_name.clone(),
            download_url: release.html_url,
            published_at: release.published_at,
        }))
    } else {
        Ok(None)
    }
}

/// Check if an update is available by comparing current version with latest GitHub release (blocking, for backwards compatibility)
pub fn check_for_update(current_version: &str) -> Result<Option<UpdateInfo>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("datatui-update-checker")
        .build()?;

    let response = client.get(GITHUB_RELEASES_API).send()?;
    
    if !response.status().is_success() {
        return Ok(None);
    }

    let release: GitHubRelease = response.json()?;
    
    // Extract version from tag_name (format: v0.3.0)
    let latest_version = release.tag_name.trim_start_matches('v');
    let current_version_clean = current_version.trim_start_matches('v');

    if is_newer_version(latest_version, current_version_clean) {
        Ok(Some(UpdateInfo {
            latest_version: release.tag_name.clone(),
            download_url: release.html_url,
            published_at: release.published_at,
        }))
    } else {
        Ok(None)
    }
}

/// Compare two version strings (format: x.y.z)
/// Returns true if latest is newer than current
fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse_version = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() >= 3 {
            Some((
                parts[0].parse().ok()?,
                parts[1].parse().ok()?,
                parts[2].parse().ok()?,
            ))
        } else {
            None
        }
    };

    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);

    match (latest_parts, current_parts) {
        (Some((l_maj, l_min, l_pat)), Some((c_maj, c_min, c_pat))) => {
            l_maj > c_maj
                || (l_maj == c_maj && l_min > c_min)
                || (l_maj == c_maj && l_min == c_min && l_pat > c_pat)
        }
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub download_url: String,
    pub published_at: String,
}

/// Calculate the next update check date (1 day from now)
pub fn calculate_next_check_date() -> DateTime<Utc> {
    Utc::now() + Duration::days(1)
}

/// Check if it's time to check for updates based on the next_check_date
pub fn should_check_for_updates(next_check_date: Option<DateTime<Utc>>) -> bool {
    match next_check_date {
        None => false, // Update checks disabled
        Some(date) => Utc::now() >= date,
    }
}
