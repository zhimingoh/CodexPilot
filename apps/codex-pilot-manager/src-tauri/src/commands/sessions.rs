use super::super::*;

#[tauri::command]
pub(crate) async fn recycle_bin_snapshot() -> Result<RecycleBinSnapshot, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(
            codex_pilot_core::app_paths::codex_state_db_path(),
        );
        adapter
            .list_undo_backups()
            .map(|entries| RecycleBinSnapshot { entries })
            .map_err(|error| format!("读取回收站失败：{error}"))
    })
    .await
    .map_err(|error| format!("读取回收站任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn restore_recycle_bin_entries(
    request: RecycleBinTokensRequest,
) -> Result<RecycleBinBatchResponse, String> {
    let tokens = sanitized_recycle_tokens(request.tokens)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(
            codex_pilot_core::app_paths::codex_state_db_path(),
        );
        let mut succeeded_tokens = Vec::new();
        let mut failed = Vec::new();
        for token in tokens {
            match adapter.undo(&token) {
                Ok(result) if result.status == codex_pilot_data::storage::DeleteStatus::Undone => {
                    succeeded_tokens.push(token);
                }
                Ok(result) => failed.push(RecycleBinBatchFailure {
                    token,
                    message: format!("{}：{}", result.session_id, result.message),
                }),
                Err(error) => failed.push(RecycleBinBatchFailure {
                    token: token.clone(),
                    message: format!("{token}：{error}"),
                }),
            }
        }
        let message = if failed.is_empty() {
            format!("已恢复 {} 条会话。", succeeded_tokens.len())
        } else {
            format!(
                "已恢复 {restored} 条，会有 {} 条失败：{}",
                failed.len(),
                failed
                    .iter()
                    .map(|item| item.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；"),
                restored = succeeded_tokens.len()
            )
        };
        Ok(RecycleBinBatchResponse {
            message,
            succeeded_tokens,
            failed,
        })
    })
    .await
    .map_err(|error| format!("恢复回收站任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn delete_recycle_bin_entries(
    request: RecycleBinTokensRequest,
) -> Result<RecycleBinBatchResponse, String> {
    let tokens = sanitized_recycle_tokens(request.tokens)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(
            codex_pilot_core::app_paths::codex_state_db_path(),
        );
        let mut succeeded_tokens = Vec::new();
        let mut failed = Vec::new();
        for token in tokens {
            match adapter.delete_undo_backup(&token) {
                Ok(result) if result.status == codex_pilot_data::storage::DeleteStatus::Deleted => {
                    succeeded_tokens.push(token);
                }
                Ok(result) => failed.push(RecycleBinBatchFailure {
                    token,
                    message: format!("{}：{}", result.session_id, result.message),
                }),
                Err(error) => failed.push(RecycleBinBatchFailure {
                    token: token.clone(),
                    message: format!("{token}：{error}"),
                }),
            }
        }
        let message = if failed.is_empty() {
            format!("已永久删除 {} 条回收站记录。", succeeded_tokens.len())
        } else {
            format!(
                "已永久删除 {deleted} 条，会有 {} 条失败：{}",
                failed.len(),
                failed
                    .iter()
                    .map(|item| item.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；"),
                deleted = succeeded_tokens.len()
            )
        };
        Ok(RecycleBinBatchResponse {
            message,
            succeeded_tokens,
            failed,
        })
    })
    .await
    .map_err(|error| format!("永久删除回收站记录任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn export_session_zip(
    request: SessionZipExportRequest,
) -> Result<SessionZipExportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let service = codex_pilot_data::session_zip::SessionZipService::new(
            codex_pilot_core::app_paths::codex_home_dir(),
        );
        let zip_path = PathBuf::from(request.zip_path);
        service
            .export_current_state_to_path(&zip_path)
            .map(|result| SessionZipExportResult {
                zip_path: result.zip_path.to_string_lossy().to_string(),
                manifest: result.manifest,
            })
            .map_err(|error| format!("导出 ZIP 失败：{error}"))
    })
    .await
    .map_err(|error| format!("导出 ZIP 任务失败：{error}"))?
}

#[tauri::command]
pub(crate) fn pick_session_zip_save_path() -> Result<Option<String>, String> {
    let file = rfd::FileDialog::new()
        .add_filter("ZIP", &["zip"])
        .set_file_name(&format!("codex-sessions-backup-{}.zip", now_secs()))
        .set_title("保存 Codex 对话备份 ZIP")
        .save_file();
    Ok(file.map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
pub(crate) fn pick_session_zip_file() -> Result<Option<String>, String> {
    let file = rfd::FileDialog::new()
        .add_filter("ZIP", &["zip"])
        .set_title("选择 Codex 对话备份 ZIP")
        .pick_file();
    Ok(file.map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
pub(crate) async fn inspect_session_zip(
    zip_path: String,
) -> Result<SessionZipInspectResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let service = codex_pilot_data::session_zip::SessionZipService::new(
            codex_pilot_core::app_paths::codex_home_dir(),
        );
        service
            .inspect_zip(Path::new(&zip_path))
            .map(|result| SessionZipInspectResult {
                zip_path: result.zip_path.to_string_lossy().to_string(),
                manifest: result.manifest,
                entries: result.entries,
            })
            .map_err(|error| format!("检查 ZIP 失败：{error}"))
    })
    .await
    .map_err(|error| format!("检查 ZIP 任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn import_session_zip(
    request: SessionZipImportRequest,
) -> Result<SessionZipImportResult, String> {
    let mode = parse_session_zip_import_mode(&request.mode)?;
    tauri::async_runtime::spawn_blocking(move || {
        let service = codex_pilot_data::session_zip::SessionZipService::new(
            codex_pilot_core::app_paths::codex_home_dir(),
        );
        service
            .import_zip(Path::new(&request.zip_path), mode)
            .map(|result| SessionZipImportResult {
                mode: match result.mode {
                    codex_pilot_data::session_zip::SessionZipImportMode::Merge => "merge",
                    codex_pilot_data::session_zip::SessionZipImportMode::Overwrite => "overwrite",
                }
                .to_string(),
                manifest: result.manifest,
                restored_session_files: result.restored_session_files,
                restored_archived_session_files: result.restored_archived_session_files,
                restored_state_sqlite: result.restored_state_sqlite,
                safety_backup_zip_path: result
                    .safety_backup_zip_path
                    .map(|path| path.to_string_lossy().to_string()),
                message: result.message,
            })
            .map_err(|error| format!("导入 ZIP 失败：{error}"))
    })
    .await
    .map_err(|error| format!("导入 ZIP 任务失败：{error}"))?
}

fn sanitized_recycle_tokens(tokens: Vec<String>) -> Result<Vec<String>, String> {
    let mut sanitized = Vec::new();
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if token.contains('/') || token.contains('\\') || token.contains("..") {
            return Err("回收站记录标识无效。".to_string());
        }
        if !sanitized.iter().any(|item| item == token) {
            sanitized.push(token.to_string());
        }
    }
    if sanitized.is_empty() {
        return Err("请选择回收站记录。".to_string());
    }
    Ok(sanitized)
}
