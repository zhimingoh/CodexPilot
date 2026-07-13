import * as React from "react";
import { CheckCircle2, Download, History, RefreshCw, Trash2 } from "lucide-react";
import { callBackend } from "../backend";
import { Distribution, Metric } from "../components/primitives";
import {
  currentProviderSyncCommand,
  dialogSyncAction,
  dialogSyncPending,
  runDialogSyncCycle,
  shouldRefreshDialogSync,
  type DialogSyncLoadState,
  type DialogSyncRefreshTrigger,
} from "../dialogSync";
import { recycleBinEntries } from "../recycleBinSupport";
import type {
  ProviderSyncSnapshot,
  RecycleBinBatchResponse,
  RecycleBinEntry,
  RecycleBinSnapshot,
  SessionZipExportResult,
  SessionZipImportMode,
  SessionZipImportResult,
  SessionZipInspectResult,
} from "../types";

export function RecycleBinView({
  recycleBin,
  syncRefreshVersion,
  onMessage,
  onProgress,
  onRefresh,
}: {
  recycleBin: RecycleBinSnapshot | null;
  syncRefreshVersion: number;
  onMessage: (message: string) => void;
  onProgress: (message: string) => void;
  onRefresh: () => void;
}) {
  const entries = recycleBinEntries(recycleBin);
  const [selected, setSelected] = React.useState<string[]>([]);
  const [pendingAction, setPendingAction] = React.useState("");
  const [zipBusy, setZipBusy] = React.useState<"" | "export" | "inspect" | "import">("");
  const [zipInspect, setZipInspect] = React.useState<SessionZipInspectResult | null>(null);
  const [zipImportMode, setZipImportMode] = React.useState<SessionZipImportMode | "">("");
  const [zipOverwriteConfirm, setZipOverwriteConfirm] = React.useState(false);
  const [syncSnapshot, setSyncSnapshot] = React.useState<ProviderSyncSnapshot | null>(null);
  const [syncLoadState, setSyncLoadState] = React.useState<DialogSyncLoadState>("loading");
  const [syncBusy, setSyncBusy] = React.useState(false);
  const syncBusyRef = React.useRef(false);
  const syncRefreshRef = React.useRef<Promise<ProviderSyncSnapshot> | null>(null);
  const [deleteConfirming, setDeleteConfirming] = React.useState(false);
  const selectedEntries = entries.filter((entry) => selected.includes(entry.token));
  const recoverableSelected = selectedEntries.filter((entry) => entry.recoverable);
  const allSelected = entries.length > 0 && selected.length === entries.length;
  const refreshProviderSync = React.useCallback(() => {
    if (syncRefreshRef.current) return syncRefreshRef.current;
    setSyncLoadState("loading");
    const request = callBackend<ProviderSyncSnapshot>("provider_sync_snapshot")
      .then((snapshot) => {
        setSyncSnapshot(snapshot);
        setSyncLoadState("ready");
        return snapshot;
      })
      .catch((error) => {
        setSyncSnapshot(null);
        setSyncLoadState("failed");
        throw error;
      })
      .finally(() => {
        syncRefreshRef.current = null;
      });
    syncRefreshRef.current = request;
    return request;
  }, []);

  const requestProviderSyncRefresh = React.useCallback((trigger: DialogSyncRefreshTrigger) => {
    if (!shouldRefreshDialogSync(trigger, document.visibilityState, syncBusyRef.current)) return;
    refreshProviderSync()
      .catch((error) => onMessage(`检查对话同步失败：${String(error)}`));
  }, [onMessage, refreshProviderSync]);

  React.useEffect(() => {
    setSelected((current) => current.filter((token) => entries.some((entry) => entry.token === token)));
  }, [entries]);

  React.useEffect(() => {
    setDeleteConfirming(false);
  }, [selected.length, entries]);

  React.useEffect(() => {
    requestProviderSyncRefresh("mount");
    const handleFocus = () => requestProviderSyncRefresh("focus");
    const handleVisibility = () => requestProviderSyncRefresh("visibility");
    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [requestProviderSyncRefresh]);

  React.useEffect(() => {
    if (syncRefreshVersion > 0) requestProviderSyncRefresh("manual");
  }, [requestProviderSyncRefresh, syncRefreshVersion]);

  React.useEffect(() => {
    setZipOverwriteConfirm(false);
  }, [zipImportMode, zipInspect?.zipPath]);

  const toggleAll = () => {
    setSelected(allSelected ? [] : entries.map((entry) => entry.token));
  };

  const toggleOne = (token: string) => {
    setSelected((current) =>
      current.includes(token)
        ? current.filter((item) => item !== token)
        : [...current, token]
    );
  };

  const restoreSelected = () => {
    if (!recoverableSelected.length || pendingAction) return;
    const tokens = recoverableSelected.map((entry) => entry.token);
    setPendingAction("restore");
    onProgress("正在恢复回收站会话");
    onMessage(`正在恢复 ${tokens.length} 条会话`);
    callBackend<RecycleBinBatchResponse>("restore_recycle_bin_entries", { request: { tokens } })
      .then((result) => {
        onMessage(result.message);
        const succeeded = new Set(result.succeededTokens);
        setSelected((current) => current.filter((token) => !succeeded.has(token)));
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPendingAction("");
        onProgress("");
      });
  };

  const deleteSelected = () => {
    if (!selected.length || pendingAction) return;
    if (!deleteConfirming) {
      setDeleteConfirming(true);
      onMessage(`再次点击"永久删除"以确认删除 ${selected.length} 条记录（不可恢复）`);
      return;
    }
    setDeleteConfirming(false);
    setPendingAction("delete");
    onProgress("正在永久删除回收站记录");
    onMessage(`正在永久删除 ${selected.length} 条记录`);
    callBackend<RecycleBinBatchResponse>("delete_recycle_bin_entries", { request: { tokens: selected } })
      .then((result) => {
        onMessage(result.message);
        const succeeded = new Set(result.succeededTokens);
        setSelected((current) => current.filter((token) => !succeeded.has(token)));
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setPendingAction("");
        onProgress("");
      });
  };

  const exportSessionZip = () => {
    if (zipBusy) return;
    setZipBusy("export");
    onProgress("正在选择并导出对话 ZIP");
    onMessage("请选择 ZIP 保存位置");
    callBackend<string | null>("pick_session_zip_save_path")
      .then((path) => {
        if (!path) {
          onMessage("已取消导出 ZIP");
          return null;
        }
        onProgress("正在导出对话 ZIP");
        onMessage("正在导出当前本地对话库");
        return callBackend<SessionZipExportResult>("export_session_zip", {
          request: {
            zipPath: path,
          },
        });
      })
      .then((result) => {
        if (!result) return;
        onMessage(`导出完成：${result.zipPath}`);
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setZipBusy("");
        onProgress("");
      });
  };

  const pickAndInspectSessionZip = () => {
    if (zipBusy) return;
    setZipBusy("inspect");
    onProgress("正在选择并检查 ZIP");
    callBackend<string | null>("pick_session_zip_file")
      .then((path) => {
        if (!path) {
          onMessage("已取消选择 ZIP");
          return null;
        }
        return callBackend<SessionZipInspectResult>("inspect_session_zip", { zipPath: path });
      })
      .then((result) => {
        if (!result) return;
        setZipInspect(result);
        setZipImportMode("");
        onMessage(`已读取 ZIP：${result.zipPath}`);
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setZipBusy("");
        onProgress("");
      });
  };

  const runSessionZipImport = () => {
    if (!zipInspect || !zipImportMode || zipBusy) return;
    if (zipImportMode === "overwrite" && !zipOverwriteConfirm) {
      setZipOverwriteConfirm(true);
      onMessage("请再确认一次覆盖恢复。执行前会先创建本地安全备份 ZIP。");
      return;
    }
    setZipBusy("import");
    onProgress(zipImportMode === "merge" ? "正在合并导入 ZIP" : "正在覆盖恢复 ZIP");
    onMessage(zipImportMode === "merge" ? "正在合并导入对话 ZIP" : "正在覆盖恢复对话 ZIP");
    callBackend<SessionZipImportResult>("import_session_zip", {
      request: {
        zipPath: zipInspect.zipPath,
        mode: zipImportMode,
      },
    })
      .then((result) => {
        onMessage(result.message);
        setZipOverwriteConfirm(false);
        onRefresh();
      })
      .catch((error) => onMessage(String(error)))
      .finally(() => {
        setZipBusy("");
        onProgress("");
      });
  };

  const runProviderSync = () => {
    if (
      !syncSnapshot
      || syncBusyRef.current
      || syncRefreshRef.current
      || syncLoadState !== "ready"
      || dialogSyncPending(syncSnapshot) <= 0
    ) return;
    syncBusyRef.current = true;
    setSyncBusy(true);
    onProgress("正在同步对话");
    onMessage("正在把全部对话同步到当前 Provider");
    const action = currentProviderSyncCommand();
    runDialogSyncCycle(
      () => callBackend<string>(action.command),
      () => action.refreshScope === "dialog-sync" ? refreshProviderSync() : Promise.resolve(),
    )
      .then(({ syncResult, syncError, refreshError }) => {
        if (syncResult) onMessage(syncResult);
        if (syncError) onMessage(`同步对话失败：${String(syncError)}`);
        if (refreshError) onMessage(`刷新对话同步状态失败：${String(refreshError)}`);
      })
      .finally(() => {
        syncBusyRef.current = false;
        setSyncBusy(false);
        onProgress("");
      });
  };

  const syncPending = syncSnapshot ? dialogSyncPending(syncSnapshot) : 0;
  const syncAction = dialogSyncAction(syncSnapshot, syncBusy, syncLoadState);
  const syncStatusTitle = syncLoadState === "loading"
    ? "正在检查历史会话"
    : syncLoadState === "failed"
    ? "无法检查历史会话"
    : !syncSnapshot
      ? "正在检查历史会话"
    : syncPending > 0
      ? "历史会话需要同步"
      : "历史会话记录一致";
  const syncStatusDetail = syncLoadState === "loading"
    ? "正在读取当前 Provider 和本机对话记录。"
    : syncLoadState === "failed"
    ? "检查暂时失败，请重新检查。"
    : !syncSnapshot
      ? "正在读取当前 Provider 和本机对话记录。"
    : syncPending > 0
      ? `预计更新 ${syncSnapshot.rolloutRewriteNeeded} 个原始会话文件、${syncSnapshot.sqliteProviderRowsNeedingSync} 条本地索引记录。`
      : `${syncSnapshot.rolloutFiles} 个原始会话文件、${syncSnapshot.sqliteRows} 条本地索引记录已对齐。`;

  const recycleTooltip = (entry: RecycleBinEntry) => [
    `标题：${entry.title || "未命名会话"}`,
    `项目：${entry.projectCwd || "未知项目"}`,
    `会话 ID：${entry.sessionId || "-"}`,
    `备份：${entry.backupPath || "-"}`,
    `类型：${schemaLabel(entry.schema)}`,
    `状态：${entry.status || (entry.recoverable ? "可恢复" : "不可恢复")}`,
  ].join("\n");

  const mergeDisabled = !zipInspect || zipBusy === "import";
  const overwriteDisabled = !zipInspect || zipBusy === "import";
  const canRunZipImport =
    Boolean(zipInspect) &&
    Boolean(zipImportMode) &&
    (zipImportMode !== "overwrite" || zipOverwriteConfirm);

  return (
    <div className="sessionsLayout">
    <section className="panel recyclePanel">
      <div className="panelHeader">
        <div className="panelTitle">
          <Trash2 size={16} />
          <h2>回收站</h2>
        </div>
        <div className="buttonRow">
          <button className="secondary" onClick={onRefresh} type="button">刷新</button>
          <button
            className="secondary"
            disabled={!recoverableSelected.length || Boolean(pendingAction)}
            onClick={restoreSelected}
            type="button"
          >
            {pendingAction === "restore" ? "恢复中" : "恢复可恢复项"}
          </button>
          <button
            className="dangerButton"
            disabled={!selected.length || Boolean(pendingAction)}
            onClick={deleteSelected}
            type="button"
          >
            {pendingAction === "delete"
              ? "删除中"
              : deleteConfirming
                ? "再次点击确认"
                : "永久删除"}
          </button>
        </div>
      </div>
      <p className="formHint">
        共 {entries.length} 条记录，已选择 {selected.length} 条。永久删除只删除恢复备份，删除后不能恢复。
      </p>
      {entries.length ? (
        <div className="tableWrap">
          <table className="dataTable">
            <thead>
              <tr>
                <th>
                  <input
                    checked={allSelected}
                    onChange={toggleAll}
                    type="checkbox"
                    aria-label="选择全部回收站记录"
                  />
                </th>
                <th>标题</th>
                <th>来源</th>
                <th>最后活跃</th>
                <th>删除时间</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => {
                const title = entry.title || "未命名会话";
                const project = projectLabel(entry.projectCwd);
                const deletedAt = formatTimestamp(entry.deletedAt);
                const lastActiveAt = formatTimestamp(entry.lastActiveAt);
                return (
                  <tr key={entry.token} title={recycleTooltip(entry)}>
                    <td>
                      <input
                        checked={selected.includes(entry.token)}
                        onChange={() => toggleOne(entry.token)}
                        type="checkbox"
                        aria-label={`选择 ${title}`}
                      />
                    </td>
                    <td>
                      <span className="cellText strong" title={title}>{title}</span>
                    </td>
                    <td>
                      <span className="cellText" title={entry.projectCwd || project}>{project}</span>
                    </td>
                    <td>
                      <span className="cellText mono" title={lastActiveAt}>{lastActiveAt}</span>
                    </td>
                    <td>
                      <span className="cellText mono" title={deletedAt}>{deletedAt}</span>
                    </td>
                    <td>
                      <span className={`pill ${entry.recoverable ? "ok" : "warning"}`} title={entry.status}>
                        {entry.status || (entry.recoverable ? "可恢复" : "不可恢复")}
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="bodyText">暂无已删除会话。</p>
      )}
      <div className="sessionZipInlineBar">
        <div className="sessionZipInlineCopy">
          <div className="panelTitle compactTitle">
            <Download size={15} />
            <h3>备份与恢复</h3>
          </div>
          <p className="formHint">
            导出当前对话库为 ZIP，或导入已有 ZIP。导入前需要明确选择“合并导入”或“覆盖恢复”。
          </p>
        </div>
        <div className="buttonRow">
          <button className="secondary" disabled={Boolean(zipBusy)} onClick={pickAndInspectSessionZip} type="button">
            <Download size={16} />
            {zipBusy === "inspect" ? "检查中" : "导入 ZIP"}
          </button>
          <button className="primary" disabled={Boolean(zipBusy)} onClick={exportSessionZip} type="button">
            <Download size={16} />
            {zipBusy === "export" ? "导出中" : "导出 ZIP"}
          </button>
        </div>
      </div>
      {zipInspect ? (
        <div className="sessionZipPanel">
          <div className="zipInspectCard">
            <div className="zipInspectHeader">
              <strong>{zipInspect.zipPath}</strong>
              <span className="pill info">ZIP 已就绪</span>
            </div>
            <dl className="metricGrid overviewMetrics">
              <Metric label="导出时间" value={formatTimestampMs(zipInspect.manifest.exportedAtMs)} />
              <Metric label="sessions" value={zipInspect.entries.sessions ? `${zipInspect.manifest.counts.sessionFiles} 个文件` : "未包含"} />
              <Metric
                label="archived_sessions"
                value={zipInspect.entries.archivedSessions ? `${zipInspect.manifest.counts.archivedSessionFiles} 个文件` : "未包含"}
              />
              <Metric label="state_5.sqlite" value={zipInspect.entries.stateSqlite ? "已包含" : "未包含"} />
            </dl>
            <div className="zipModeGrid">
              <button
                className={`modeCard ${zipImportMode === "merge" ? "active" : ""}`}
                disabled={mergeDisabled}
                onClick={() => setZipImportMode("merge")}
                type="button"
              >
                <strong>合并导入</strong>
                <span>恢复 ZIP 里的会话文件；不会替换当前 `state_5.sqlite`。</span>
              </button>
              <button
                className={`modeCard dangerMode ${zipImportMode === "overwrite" ? "active" : ""}`}
                disabled={overwriteDisabled}
                onClick={() => setZipImportMode("overwrite")}
                type="button"
              >
                <strong>覆盖恢复</strong>
                <span>按 ZIP 覆盖当前本地会话目录；执行前会先创建一份本地安全备份 ZIP。</span>
              </button>
            </div>
            {zipImportMode === "merge" && (
              <div className="inlineNotice">
                <span className="statusDot ok" />
                <span>
                  当前选择“合并导入”。即使 ZIP 包含 `state_5.sqlite`，这次也不会导入数据库文件。
                </span>
              </div>
            )}
            {zipImportMode === "overwrite" && (
              <div className="overwriteConfirmCard">
                <div className="inlineNotice warning">
                  <span className="statusDot warning" />
                  <span>
                    当前选择“覆盖恢复”。它会替换当前本地对话目录，并在 ZIP 包含时替换 `state_5.sqlite`。
                  </span>
                </div>
                <label className="checkboxRow compactCheckbox">
                  <input
                    checked={zipOverwriteConfirm}
                    onChange={(event) => setZipOverwriteConfirm(event.target.checked)}
                    type="checkbox"
                  />
                  <span>我确认执行前先创建本地安全备份，然后再覆盖恢复。</span>
                </label>
              </div>
            )}
            <div className="buttonRow">
              <button
                className={zipImportMode === "overwrite" ? "dangerButton" : "primary"}
                disabled={!canRunZipImport || zipBusy === "import"}
                onClick={runSessionZipImport}
                type="button"
              >
                {zipBusy === "import"
                  ? zipImportMode === "overwrite"
                    ? "恢复中"
                    : "导入中"
                  : zipImportMode === "overwrite"
                    ? "执行覆盖恢复"
                    : "执行合并导入"}
              </button>
              <button
                className="secondary"
                disabled={Boolean(zipBusy)}
                onClick={() => {
                  setZipInspect(null);
                  setZipImportMode("");
                  setZipOverwriteConfirm(false);
                }}
                type="button"
              >
                清除当前 ZIP
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
    <section className="panel">
      <div className="panelHeader">
        <div className="panelTitle">
          <History size={16} />
          <h2>对话同步</h2>
        </div>
      </div>
      <div className="syncTool">
        <div className={`syncStatusCard ${syncAction.kind}`}>
          <div className="syncStatusMain">
            <div className="syncStatusCopy">
              <span className="syncStatusIcon">
                {syncAction.kind === "synced" ? <CheckCircle2 size={16} /> : <RefreshCw size={16} />}
              </span>
              <div>
                <strong>{syncStatusTitle}</strong>
                <p>{syncStatusDetail}</p>
              </div>
            </div>
            <button
              className={syncAction.kind === "ready" ? "primary" : "secondary"}
              disabled={syncAction.disabled}
              onClick={syncAction.kind === "failed"
                ? () => requestProviderSyncRefresh("manual")
                : runProviderSync}
              type="button"
            >
              {syncAction.kind === "synced" ? <CheckCircle2 size={16} /> : <RefreshCw size={16} />}
              {syncAction.label}
            </button>
          </div>
          <dl>
            <Metric label="当前 Provider" value={syncSnapshot?.currentProvider ?? "-"} />
          </dl>
        </div>
        <details className="syncDetails">
          <summary>查看技术详情</summary>
          <div className="syncSummaryGrid">
            <Metric label="原始文件待改" value={`${syncSnapshot?.rolloutRewriteNeeded ?? 0}/${syncSnapshot?.rolloutFiles ?? 0}`} />
            <Metric label="本地索引待改" value={`${syncSnapshot?.sqliteProviderRowsNeedingSync ?? 0}/${syncSnapshot?.sqliteRows ?? 0}`} />
          </div>
          <div className="providerDistribution">
            <Distribution title="原始会话文件分布" items={syncSnapshot?.rolloutProviders ?? []} />
            <Distribution title="本地索引记录分布" items={syncSnapshot?.sqliteProviders ?? []} />
          </div>
        </details>
      </div>
    </section>
    </div>
  );
}

function formatTimestamp(value: number | null) {
  if (!value) return "-";
  return new Date(value * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatTimestampMs(value: number | null) {
  if (!value) return "-";
  return new Date(value).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function projectLabel(cwd: string | null) {
  if (!cwd) return "未知项目";
  const normalized = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] || cwd;
}

function schemaLabel(schema: string) {
  if (schema === "codex_threads") return "Codex 对话";
  if (schema === "generic_sessions") return "旧版会话";
  return schema || "未知";
}
