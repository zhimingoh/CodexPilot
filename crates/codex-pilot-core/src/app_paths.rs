use std::ffi::OsStr;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
static TEST_APP_STATE_DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

#[cfg(test)]
static TEST_CODEX_HOME_DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

#[cfg(test)]
static TEST_PATHS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DesktopHostKind {
    LegacyCodex,
    ChatGptUnified,
}

impl DesktopHostKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::LegacyCodex => "legacy_codex",
            Self::ChatGptUnified => "chatgpt_unified",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::LegacyCodex => "Codex",
            Self::ChatGptUnified => "ChatGPT",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolvedDesktopHost {
    pub kind: DesktopHostKind,
    pub app_dir: PathBuf,
    pub executable: PathBuf,
}

pub fn codex_home_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(path) = TEST_CODEX_HOME_DIR
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|value| value.clone()))
    {
        return path;
    }

    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".codex"))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

pub fn codex_config_path() -> PathBuf {
    codex_home_dir().join("config.toml")
}

pub fn codex_auth_path() -> PathBuf {
    codex_home_dir().join("auth.json")
}

pub fn codex_state_db_path() -> PathBuf {
    codex_home_dir().join("state_5.sqlite")
}

pub fn app_state_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(path) = TEST_APP_STATE_DIR
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|value| value.clone()))
    {
        return path;
    }

    if cfg!(windows) {
        if let Some(app_data) = std::env::var_os("APPDATA") {
            return PathBuf::from(app_data).join("CodexPilot");
        }
    }

    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("CodexPilot")
}

#[cfg(test)]
pub fn set_test_app_state_dir(path: Option<PathBuf>) {
    let slot = TEST_APP_STATE_DIR.get_or_init(|| Mutex::new(None));
    if let Ok(mut value) = slot.lock() {
        *value = path;
    }
}

#[cfg(test)]
pub fn set_test_codex_home_dir(path: Option<PathBuf>) {
    let slot = TEST_CODEX_HOME_DIR.get_or_init(|| Mutex::new(None));
    if let Ok(mut value) = slot.lock() {
        *value = path;
    }
}

#[cfg(test)]
pub fn test_paths_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_PATHS_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn resolve_codex_app_dir(app_dir: Option<&Path>) -> Option<PathBuf> {
    resolve_codex_host(app_dir).map(|host| host.app_dir)
}

pub fn resolve_codex_host(app_dir: Option<&Path>) -> Option<ResolvedDesktopHost> {
    if let Some(app_dir) = app_dir {
        return resolve_host_from_path(app_dir);
    }
    if cfg!(target_os = "macos") {
        find_macos_codex_host_default()
    } else {
        find_latest_codex_host_default()
    }
}

pub fn resolve_host_from_path(path: &Path) -> Option<ResolvedDesktopHost> {
    if path.is_file() {
        let kind = executable_host_kind(path)?;
        return Some(ResolvedDesktopHost {
            kind,
            app_dir: path.parent().unwrap_or(path).to_path_buf(),
            executable: path.to_path_buf(),
        });
    }
    if path.extension() == Some(OsStr::new("app")) {
        let executable = build_codex_executable(path);
        return Some(ResolvedDesktopHost {
            kind: if executable
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.eq_ignore_ascii_case("ChatGPT"))
            {
                DesktopHostKind::ChatGptUnified
            } else {
                DesktopHostKind::LegacyCodex
            },
            app_dir: path.to_path_buf(),
            executable,
        });
    }
    let executable = build_codex_executable(path);
    let kind = executable_host_kind(&executable)?;
    Some(ResolvedDesktopHost {
        kind,
        app_dir: path.to_path_buf(),
        executable,
    })
}

pub fn build_codex_executable(app_dir: &Path) -> PathBuf {
    if app_dir.is_file() {
        return app_dir.to_path_buf();
    }
    if app_dir.extension() == Some(OsStr::new("app")) {
        let chatgpt = app_dir.join("Contents").join("MacOS").join("ChatGPT");
        if chatgpt.exists() {
            return chatgpt;
        }
        return app_dir.join("Contents").join("MacOS").join("Codex");
    }
    let chatgpt = app_dir.join("ChatGPT.exe");
    if chatgpt.exists() {
        return chatgpt;
    }
    let upper = app_dir.join("Codex.exe");
    if upper.exists() {
        upper
    } else {
        app_dir.join("codex.exe")
    }
}

pub fn find_macos_codex_app_default() -> Option<PathBuf> {
    find_macos_codex_host_default().map(|host| host.app_dir)
}

pub fn find_macos_codex_host_default() -> Option<ResolvedDesktopHost> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        roots.push(home.join("Applications"));
    }
    find_macos_codex_host(&roots)
}

pub fn find_macos_codex_app(search_roots: &[PathBuf]) -> Option<PathBuf> {
    find_macos_codex_host(search_roots).map(|host| host.app_dir)
}

pub fn find_macos_codex_host(search_roots: &[PathBuf]) -> Option<ResolvedDesktopHost> {
    for root in search_roots {
        for candidate in macos_app_candidates(root) {
            if is_macos_codex_app(&candidate) {
                return resolve_host_from_path(&candidate);
            }
        }
    }
    None
}

pub fn find_latest_codex_app_dir_default() -> Option<PathBuf> {
    find_latest_codex_host_default().map(|host| host.app_dir)
}

pub fn find_latest_codex_host_default() -> Option<ResolvedDesktopHost> {
    #[cfg(windows)]
    {
        find_latest_codex_host_from_roots(&windows_app_package_roots())
    }

    #[cfg(not(windows))]
    {
        None
    }
}

pub fn find_latest_codex_app_dir_from_roots(roots: &[PathBuf]) -> Option<PathBuf> {
    find_latest_codex_host_from_roots(roots).map(|host| host.app_dir)
}

pub fn find_latest_codex_host_from_roots(roots: &[PathBuf]) -> Option<ResolvedDesktopHost> {
    roots
        .iter()
        .filter_map(|root| find_latest_codex_host(root))
        .max_by(|left, right| {
            version_tuple(left.app_dir.parent().unwrap_or(&left.app_dir)).cmp(&version_tuple(
                right.app_dir.parent().unwrap_or(&right.app_dir),
            ))
        })
}

pub fn find_latest_codex_app_dir(root: &Path) -> Option<PathBuf> {
    find_latest_codex_host(root).map(|host| host.app_dir)
}

pub fn find_latest_codex_host(root: &Path) -> Option<ResolvedDesktopHost> {
    let mut matches = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| version_tuple(&path).map(|version| (version, path)))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.0.cmp(&right.0));
    let (_, latest) = matches.pop()?;
    let app = latest.join("app");
    let app_dir = if app.is_dir() { app } else { latest };
    resolve_host_from_path(&app_dir)
}

#[cfg(windows)]
fn windows_app_package_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files).join("WindowsApps"));
    }
    if let Some(program_files) = std::env::var_os("ProgramW6432") {
        roots.push(PathBuf::from(program_files).join("WindowsApps"));
    }
    roots.push(PathBuf::from(r"C:\Program Files\WindowsApps"));
    roots.sort();
    roots.dedup();
    roots
}

fn macos_app_candidates(root: &Path) -> Vec<PathBuf> {
    if root.extension() == Some(OsStr::new("app")) {
        return vec![root.to_path_buf()];
    }
    [
        "ChatGPT.app",
        "OpenAI ChatGPT.app",
        "OpenAI.ChatGPT.app",
        "Codex.app",
        "OpenAI Codex.app",
        "OpenAI.Codex.app",
    ]
    .into_iter()
    .map(|name| root.join(name))
    .collect()
}

fn executable_host_kind(path: &Path) -> Option<DesktopHostKind> {
    let name = path.file_name()?.to_str()?;
    if name.eq_ignore_ascii_case("ChatGPT.exe") || name.eq_ignore_ascii_case("ChatGPT") {
        Some(DesktopHostKind::ChatGptUnified)
    } else if name.eq_ignore_ascii_case("Codex.exe")
        || name.eq_ignore_ascii_case("codex.exe")
        || name.eq_ignore_ascii_case("Codex")
    {
        Some(DesktopHostKind::LegacyCodex)
    } else {
        None
    }
}

fn is_macos_codex_app(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    if matches!(
        path.file_name().and_then(OsStr::to_str),
        Some("Codex.app" | "OpenAI Codex.app" | "OpenAI.Codex.app")
    ) {
        return true;
    }
    let info_plist = path.join("Contents").join("Info.plist");
    std::fs::read_to_string(info_plist).is_ok_and(|contents| {
        contents.contains("<string>com.openai.codex</string>")
            || contents.contains("<string>Codex</string>")
    })
}

fn version_tuple(path: &Path) -> Option<Vec<u32>> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("OpenAI.Codex_")?;
    let version = rest.split_once('_')?.0;
    let parts = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.is_empty() { None } else { Some(parts) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_chatgpt_executable_resolves_as_unified_host() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("ChatGPT.exe");
        std::fs::write(&exe, "").unwrap();

        let host = resolve_codex_host(Some(&exe)).expect("host should resolve");

        assert_eq!(host.kind, DesktopHostKind::ChatGptUnified);
        assert_eq!(host.executable, exe);
        assert_eq!(host.app_dir, temp.path());
    }

    #[test]
    fn chatgpt_directory_executable_is_preferred_when_present() {
        let temp = tempfile::tempdir().unwrap();
        let chatgpt = temp.path().join("ChatGPT.exe");
        let codex = temp.path().join("Codex.exe");
        std::fs::write(&chatgpt, "").unwrap();
        std::fs::write(&codex, "").unwrap();

        let host = resolve_codex_host(Some(temp.path())).expect("host should resolve");

        assert_eq!(host.kind, DesktopHostKind::ChatGptUnified);
        assert_eq!(host.executable, chatgpt);
    }

    #[test]
    fn legacy_codex_directory_resolves_when_chatgpt_is_absent() {
        let temp = tempfile::tempdir().unwrap();
        let codex = temp.path().join("Codex.exe");
        std::fs::write(&codex, "").unwrap();

        let host = resolve_codex_host(Some(temp.path())).expect("host should resolve");

        assert_eq!(host.kind, DesktopHostKind::LegacyCodex);
        assert_eq!(host.executable, codex);
    }

    #[test]
    fn finds_latest_chatgpt_app_dir_from_windows_roots() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp
            .path()
            .join("OpenAI.Codex_1.0.0.0_x64__abc")
            .join("app");
        let latest = temp
            .path()
            .join("OpenAI.Codex_2.0.0.0_x64__abc")
            .join("app");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&latest).unwrap();
        std::fs::write(latest.join("ChatGPT.exe"), "").unwrap();

        let host = find_latest_codex_host_from_roots(&[temp.path().to_path_buf()])
            .expect("host should resolve");

        assert_eq!(host.kind, DesktopHostKind::ChatGptUnified);
        assert_eq!(host.app_dir, latest);
    }

    #[test]
    fn finds_latest_legacy_codex_app_dir_from_windows_roots() {
        let temp = tempfile::tempdir().unwrap();
        let latest = temp
            .path()
            .join("OpenAI.Codex_3.0.0.0_x64__abc")
            .join("app");
        std::fs::create_dir_all(&latest).unwrap();
        std::fs::write(latest.join("Codex.exe"), "").unwrap();

        let host = find_latest_codex_host_from_roots(&[temp.path().to_path_buf()])
            .expect("host should resolve");

        assert_eq!(host.kind, DesktopHostKind::LegacyCodex);
        assert_eq!(host.app_dir, latest);
    }

    #[test]
    fn macos_detector_accepts_chatgpt_app_with_codex_bundle_id() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("ChatGPT.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        std::fs::write(
            app.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.openai.codex</string>
</dict>
</plist>
"#,
        )
        .unwrap();

        assert_eq!(
            find_macos_codex_app(&[root.path().to_path_buf()]),
            Some(app)
        );
    }
}
