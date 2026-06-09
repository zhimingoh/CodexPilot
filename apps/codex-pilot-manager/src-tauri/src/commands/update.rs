use super::super::*;
use serde::Deserialize;

const GITEE_UPDATE_FEED_URL: &str =
    "https://gitee.com/hl95599/CodexPilot/raw/main/docs/update/latest.json";
const JSDELIVR_UPDATE_FEED_URL: &str =
    "https://cdn.jsdelivr.net/gh/hl9565/CodexPilot@main/docs/update/latest.json";
const UPDATE_FEED_URLS: &[&str] = &[GITEE_UPDATE_FEED_URL, JSDELIVR_UPDATE_FEED_URL];
const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/hl9565/CodexPilot/releases/latest";
const RELEASE_URL_PREFIX: &str = "https://github.com/hl9565/CodexPilot/releases/";
const UPDATE_EVENT: &str = "update_state_changed";
const UPDATE_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);
const UPDATE_FEED_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
const GITHUB_API_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateSnapshot {
    current_version: String,
    latest_version: Option<String>,
    latest_tag: Option<String>,
    release_url: Option<String>,
    release_name: Option<String>,
    published_at: Option<String>,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseMetadata {
    tag_name: String,
    html_url: String,
    name: Option<String>,
    published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateFeed {
    latest_tag: String,
    latest_version: Option<String>,
    release_url: String,
    release_name: Option<String>,
    published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    name: Option<String>,
    published_at: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum VersionOrdering {
    Older,
    Equal,
    Newer,
}

#[tauri::command]
pub(crate) async fn check_latest_release(app: tauri::AppHandle) -> Result<UpdateSnapshot, String> {
    let settings = tauri::async_runtime::spawn_blocking(load_update_settings)
        .await
        .map_err(|error| format!("读取更新提醒设置任务失败：{error}"))?;

    let snapshot = match fetch_latest_release_in_task().await {
        Ok(release) => build_release_snapshot(&release, &settings),
        Err(error) => failed_snapshot(error),
    };
    emit_update_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn ignore_latest_release(
    app: tauri::AppHandle,
    tag: String,
) -> Result<UpdateSnapshot, String> {
    let sanitized = sanitize_release_tag(&tag).ok_or_else(|| "更新版本标签无效。".to_string())?;
    if normalize_version(&sanitized).is_none() {
        return Err("更新版本标签无效。".to_string());
    }
    let settings = UpdateSettings {
        ignored_update_tag: Some(sanitized),
    };
    let settings_for_save = settings.clone();
    tauri::async_runtime::spawn_blocking(move || save_update_settings(&settings_for_save))
        .await
        .map_err(|error| format!("保存更新提醒设置任务失败：{error}"))??;

    let snapshot = match fetch_latest_release_in_task().await {
        Ok(release) => build_release_snapshot(&release, &settings),
        Err(error) => failed_snapshot(error),
    };
    emit_update_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn open_release_url(url: String) -> Result<String, String> {
    let url = validate_release_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || open_url_with_system(&url))
        .await
        .map_err(|error| format!("打开发布页任务失败：{error}"))??;
    Ok("已打开发布页。".to_string())
}

async fn fetch_latest_release() -> Result<ReleaseMetadata, String> {
    let mut feed_errors = Vec::new();
    let mut feed_releases = Vec::new();

    for url in UPDATE_FEED_URLS {
        match fetch_update_feed(url).await {
            Ok(release) => feed_releases.push(release),
            Err(error) => feed_errors.push(format!("{url}: {error}")),
        }
    }

    if let Some(release) = newest_release(feed_releases) {
        return Ok(release);
    }

    match fetch_github_release().await {
        Ok(github_release) => Ok(github_release),
        Err(error) => {
            let feed_detail = if feed_errors.is_empty() {
                "静态更新源不可用".to_string()
            } else {
                format!("静态更新源不可用（{}）", feed_errors.join("；"))
            };
            Err(format!(
                "暂时无法检查更新：{feed_detail}，GitHub API 也不可用：{error}"
            ))
        }
    }
}

async fn fetch_update_feed(url: &str) -> Result<ReleaseMetadata, String> {
    let response = codex_pilot_core::http_client::shared()
        .get(url)
        .timeout(UPDATE_FEED_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("请求失败：{error}"))?;

    if !response.status().is_success() {
        return Err(format!("返回 {}", response.status()));
    }

    let feed = response
        .json::<UpdateFeed>()
        .await
        .map_err(|error| format!("解析失败：{error}"))?;

    release_from_feed(feed)
}

async fn fetch_github_release() -> Result<ReleaseMetadata, String> {
    let response = codex_pilot_core::http_client::shared()
        .get(LATEST_RELEASE_API_URL)
        .timeout(GITHUB_API_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("暂时无法检查更新：{error}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "暂时无法检查更新：GitHub 返回 {}",
            response.status()
        ));
    }

    let release = response
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("暂时无法解析更新信息：{error}"))?;

    Ok(ReleaseMetadata {
        tag_name: release.tag_name,
        html_url: release.html_url,
        name: release.name,
        published_at: release.published_at,
    })
}

async fn fetch_latest_release_in_task() -> Result<ReleaseMetadata, String> {
    tokio::time::timeout(
        UPDATE_CHECK_TIMEOUT,
        tauri::async_runtime::spawn_blocking(|| {
            tauri::async_runtime::block_on(fetch_latest_release())
        }),
    )
    .await
    .map_err(|_| "暂时无法检查更新：请求超时".to_string())?
    .map_err(|error| format!("检查更新任务失败：{error}"))?
}

fn release_from_feed(feed: UpdateFeed) -> Result<ReleaseMetadata, String> {
    let tag =
        sanitize_release_tag(&feed.latest_tag).ok_or_else(|| "更新 feed 标签为空。".to_string())?;
    let tag_version =
        normalize_version(&tag).ok_or_else(|| "更新 feed 标签格式无效。".to_string())?;
    if let Some(feed_version) = feed.latest_version.as_deref() {
        let normalized_feed_version = normalize_version(feed_version)
            .ok_or_else(|| "更新 feed 版本格式无效。".to_string())?;
        if normalized_feed_version != tag_version {
            return Err("更新 feed 版本与标签不一致。".to_string());
        }
    }

    Ok(ReleaseMetadata {
        tag_name: tag,
        html_url: validate_release_url(&feed.release_url)?,
        name: feed.release_name,
        published_at: feed.published_at,
    })
}

fn newest_release<I>(releases: I) -> Option<ReleaseMetadata>
where
    I: IntoIterator<Item = ReleaseMetadata>,
{
    let mut best: Option<ReleaseMetadata> = None;
    for release in releases {
        if release_version(&release).is_none() {
            continue;
        }
        let should_replace = best
            .as_ref()
            .and_then(|current| compare_release_versions(current, &release))
            .is_none_or(|ordering| ordering == VersionOrdering::Newer);
        if should_replace {
            best = Some(release);
        }
    }
    best
}

fn compare_release_versions(
    current: &ReleaseMetadata,
    latest: &ReleaseMetadata,
) -> Option<VersionOrdering> {
    compare_versions(release_version(current)?, release_version(latest)?)
}

fn release_version(release: &ReleaseMetadata) -> Option<&str> {
    normalize_version(release.tag_name.as_str())
}

fn build_release_snapshot(release: &ReleaseMetadata, settings: &UpdateSettings) -> UpdateSnapshot {
    let current_version = codex_pilot_core::version::VERSION.to_string();
    let latest_tag = sanitize_release_tag(&release.tag_name);
    let latest_version = latest_tag
        .as_deref()
        .and_then(normalize_version)
        .map(str::to_string);
    let comparison = latest_version
        .as_deref()
        .and_then(|latest| compare_versions(&current_version, latest));

    let status = match (&latest_tag, comparison) {
        (Some(tag), Some(VersionOrdering::Newer))
            if settings.ignored_update_tag.as_deref() == Some(tag.as_str()) =>
        {
            "ignored"
        }
        (Some(_), Some(VersionOrdering::Newer)) => "available",
        (Some(_), Some(VersionOrdering::Equal | VersionOrdering::Older)) => "latest",
        _ => "failed",
    };
    let error = if status == "failed" {
        Some("发布版本标签格式无效。".to_string())
    } else {
        None
    };

    UpdateSnapshot {
        current_version,
        latest_version,
        latest_tag,
        release_url: validate_release_url(&release.html_url).ok(),
        release_name: release.name.clone(),
        published_at: release.published_at.clone(),
        status: status.to_string(),
        error,
    }
}

fn failed_snapshot(error: String) -> UpdateSnapshot {
    UpdateSnapshot {
        current_version: codex_pilot_core::version::VERSION.to_string(),
        latest_version: None,
        latest_tag: None,
        release_url: None,
        release_name: None,
        published_at: None,
        status: "failed".to_string(),
        error: Some(error),
    }
}

fn emit_update_snapshot(app: &tauri::AppHandle, snapshot: &UpdateSnapshot) {
    use tauri::Emitter;
    let _ = app.emit(UPDATE_EVENT, snapshot);
}

fn sanitize_release_tag(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_version(value: &str) -> Option<&str> {
    let normalized = value.trim().trim_start_matches('v').trim_start_matches('V');
    if normalized.is_empty() || normalized.contains('-') || normalized.contains('+') {
        return None;
    }
    if normalized.split('.').all(|part| {
        !part.is_empty()
            && part.chars().all(|ch| ch.is_ascii_digit())
            && part.parse::<u64>().is_ok()
    }) {
        Some(normalized)
    } else {
        None
    }
}

fn compare_versions(current: &str, latest: &str) -> Option<VersionOrdering> {
    let current = parse_version_parts(current)?;
    let latest = parse_version_parts(latest)?;
    let max_len = current.len().max(latest.len());
    for index in 0..max_len {
        let left = current.get(index).copied().unwrap_or(0);
        let right = latest.get(index).copied().unwrap_or(0);
        if right > left {
            return Some(VersionOrdering::Newer);
        }
        if right < left {
            return Some(VersionOrdering::Older);
        }
    }
    Some(VersionOrdering::Equal)
}

fn parse_version_parts(value: &str) -> Option<Vec<u64>> {
    let normalized = normalize_version(value)?;
    let parts = normalized
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if parts.is_empty() { None } else { Some(parts) }
}

fn validate_release_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with(RELEASE_URL_PREFIX)
        && !trimmed.contains(char::is_whitespace)
        && !trimmed.contains('\\')
    {
        Ok(trimmed.to_string())
    } else {
        Err("发布页地址无效。".to_string())
    }
}

fn open_url_with_system(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = codex_pilot_core::windows_integration::std_command("open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = codex_pilot_core::windows_integration::std_command("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = codex_pilot_core::windows_integration::std_command("xdg-open");
        command.arg(url);
        command
    };

    let status = codex_pilot_core::windows_integration::status_hidden(&mut command)
        .map_err(|error| format!("打开发布页失败：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("打开发布页失败：{status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag_name: &str) -> ReleaseMetadata {
        ReleaseMetadata {
            tag_name: tag_name.to_string(),
            html_url: format!("{RELEASE_URL_PREFIX}tag/{tag_name}"),
            name: Some(format!("Release {tag_name}")),
            published_at: Some("2026-06-09T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn normalize_release_versions() {
        assert_eq!(normalize_version("v1.3.2"), Some("1.3.2"));
        assert_eq!(normalize_version("V1.3.2"), Some("1.3.2"));
        assert_eq!(normalize_version("1.3.2"), Some("1.3.2"));
        assert_eq!(normalize_version("1.3.2-beta.1"), None);
        assert_eq!(normalize_version("1.3.x"), None);
    }

    #[test]
    fn compare_semantic_versions() {
        assert_eq!(
            compare_versions("1.3.2", "1.3.3"),
            Some(VersionOrdering::Newer)
        );
        assert_eq!(
            compare_versions("1.3.2", "1.3.2"),
            Some(VersionOrdering::Equal)
        );
        assert_eq!(
            compare_versions("1.3.2", "1.3.1"),
            Some(VersionOrdering::Older)
        );
        assert_eq!(
            compare_versions("1.3", "1.3.0"),
            Some(VersionOrdering::Equal)
        );
        assert_eq!(
            compare_versions("1.3.9", "1.4.0"),
            Some(VersionOrdering::Newer)
        );
        assert_eq!(compare_versions("1.3.2", "1.3.3-beta.1"), None);
    }

    #[test]
    fn build_snapshot_marks_ignored_latest_tag() {
        let snapshot = build_release_snapshot(
            &release("v999.0.0"),
            &UpdateSettings {
                ignored_update_tag: Some("v999.0.0".to_string()),
            },
        );

        assert_eq!(snapshot.status, "ignored");
        assert_eq!(snapshot.latest_tag.as_deref(), Some("v999.0.0"));
    }

    #[test]
    fn build_snapshot_marks_available_newer_tag() {
        let snapshot = build_release_snapshot(&release("v999.0.0"), &UpdateSettings::default());

        assert_eq!(snapshot.status, "available");
        assert_eq!(snapshot.latest_version.as_deref(), Some("999.0.0"));
    }

    #[test]
    fn validate_release_urls_are_scoped() {
        assert!(
            validate_release_url("https://github.com/hl9565/CodexPilot/releases/tag/v1.3.2")
                .is_ok()
        );
        assert!(
            validate_release_url("https://github.com/other/project/releases/tag/v1.3.2").is_err()
        );
        assert!(
            validate_release_url("https://github.com/hl9565/CodexPilot/releases/tag/v1.3.2 && bad")
                .is_err()
        );
    }

    #[test]
    fn invalid_tags_do_not_normalize() {
        assert_eq!(normalize_version(""), None);
        assert_eq!(normalize_version("release-candidate"), None);
        assert_eq!(normalize_version("v1.3.3+build"), None);
    }

    #[test]
    fn feed_release_accepts_matching_tag_and_version() {
        let release = release_from_feed(UpdateFeed {
            latest_tag: "v1.3.3".to_string(),
            latest_version: Some("1.3.3".to_string()),
            release_url: "https://github.com/hl9565/CodexPilot/releases/tag/v1.3.3".to_string(),
            release_name: Some("CodexPilot v1.3.3".to_string()),
            published_at: Some("2026-06-09T00:00:00Z".to_string()),
        })
        .unwrap();

        assert_eq!(release.tag_name, "v1.3.3");
        assert_eq!(release_version(&release), Some("1.3.3"));
    }

    #[test]
    fn feed_release_rejects_mismatched_version() {
        let error = release_from_feed(UpdateFeed {
            latest_tag: "v1.3.3".to_string(),
            latest_version: Some("1.3.2".to_string()),
            release_url: "https://github.com/hl9565/CodexPilot/releases/tag/v1.3.3".to_string(),
            release_name: None,
            published_at: None,
        })
        .unwrap_err();

        assert!(error.contains("版本与标签不一致"));
    }

    #[test]
    fn newest_release_uses_highest_valid_version() {
        let newest =
            newest_release([release("v1.3.3"), release("v1.3.2"), release("v1.4.0")]).unwrap();

        assert_eq!(newest.tag_name, "v1.4.0");
    }
}
