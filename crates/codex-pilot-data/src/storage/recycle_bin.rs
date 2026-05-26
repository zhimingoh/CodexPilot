use crate::storage::{
    BackupPayload, DeleteResult, DeleteStatus, SQLiteStorageAdapter, backup_last_active_at,
    backup_project_cwd, backup_title, not_found,
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecycleBinEntry {
    pub token: String,
    pub session_id: String,
    pub title: Option<String>,
    pub project_cwd: Option<String>,
    pub schema: String,
    pub db_path: PathBuf,
    pub backup_path: PathBuf,
    pub deleted_at: Option<u64>,
    pub last_active_at: Option<u64>,
    pub recoverable: bool,
    pub status: String,
}

impl SQLiteStorageAdapter {
    pub fn list_undo_backups(&self) -> anyhow::Result<Vec<RecycleBinEntry>> {
        if !self.backup_dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.backup_dir)
            .with_context(|| format!("read undo backup dir {}", self.backup_dir.display()))?
        {
            let item = item?;
            let path = item.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            entries.push(self.recycle_entry_from_path(&path));
        }
        entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
        Ok(entries)
    }

    pub fn delete_undo_backup(&self, token: &str) -> anyhow::Result<DeleteResult> {
        let backup_path = self.backup_path(token)?;
        let session_id = token_session_id(token);
        if !backup_path.exists() {
            return Ok(not_found(&session_id, "回收站记录不存在"));
        }
        fs::remove_file(&backup_path)
            .with_context(|| format!("delete undo backup {}", backup_path.display()))?;
        Ok(DeleteResult {
            status: DeleteStatus::Deleted,
            session_id,
            message: "已永久删除回收站记录".to_string(),
            undo_token: Some(token.to_string()),
            backup_path: Some(backup_path),
        })
    }

    pub(super) fn recycle_entry_from_path(&self, path: &Path) -> RecycleBinEntry {
        let token = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let deleted_at = path
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        let raw = fs::read_to_string(path);
        let payload = raw
            .as_deref()
            .ok()
            .and_then(|text| serde_json::from_str::<BackupPayload>(text).ok());
        match payload {
            Some(payload) => {
                let db_matches = payload.db_path == self.db_path;
                RecycleBinEntry {
                    token,
                    session_id: payload.session_id,
                    title: backup_title(&payload.tables),
                    project_cwd: backup_project_cwd(&payload.tables),
                    schema: payload.schema,
                    db_path: payload.db_path,
                    backup_path: path.to_path_buf(),
                    deleted_at,
                    last_active_at: backup_last_active_at(&payload.tables),
                    recoverable: db_matches,
                    status: if db_matches {
                        "可恢复".to_string()
                    } else {
                        "数据库不匹配".to_string()
                    },
                }
            }
            None => RecycleBinEntry {
                session_id: token_session_id(&token),
                token,
                title: None,
                project_cwd: None,
                schema: "unknown".to_string(),
                db_path: self.db_path.clone(),
                backup_path: path.to_path_buf(),
                deleted_at,
                last_active_at: None,
                recoverable: false,
                status: "备份无法解析".to_string(),
            },
        }
    }
}

fn token_session_id(token: &str) -> String {
    token
        .rsplit_once('-')
        .map(|(session_id, _)| session_id)
        .unwrap_or(token)
        .to_string()
}
