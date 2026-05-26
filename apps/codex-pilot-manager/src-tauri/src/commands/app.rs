use super::super::*;

#[tauri::command]
pub(crate) async fn backend_status()
-> Result<Option<codex_pilot_core::status::BackendStatus>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let prefs = load_launch_preferences();
        let helper_port = launch_options_from_preferences(&prefs).helper_port;
        let helper_reachable = codex_pilot_core::ports::can_connect_loopback_port(helper_port);
        let status = codex_pilot_core::status::read_status().map_err(|error| error.to_string())?;

        if helper_reachable {
            return Ok(Some(codex_pilot_core::status::BackendStatus {
                status: "running".to_string(),
                version: codex_pilot_core::version::VERSION.to_string(),
            }));
        }

        Ok(status)
    })
    .await
    .map_err(|error| format!("读取后端状态任务失败：{error}"))?
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    codex_pilot_core::version::VERSION.to_string()
}

#[tauri::command]
pub(crate) async fn save_launch_preferences(request: LaunchPreferences) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let prefs = sanitize_launch_preferences(request)?;
        save_launch_preferences_to_path(&manager_config_path(), &prefs)?;
        Ok("启动偏好已保存。".to_string())
    })
    .await
    .map_err(|error| format!("保存启动偏好任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn enhancement_settings_snapshot() -> EnhancementSettings {
    tauri::async_runtime::spawn_blocking(load_enhancement_settings)
        .await
        .expect("enhancement_settings_snapshot task panicked")
}

#[tauri::command]
pub(crate) async fn save_enhancement_settings(
    request: EnhancementSettings,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = sanitize_enhancement_settings(request);
        save_enhancement_settings_to_path(&enhancement_settings_path(), &settings)?;
        Ok("页面增强设置已保存，重新注入后生效。".to_string())
    })
    .await
    .map_err(|error| format!("保存页面增强设置任务失败：{error}"))?
}
