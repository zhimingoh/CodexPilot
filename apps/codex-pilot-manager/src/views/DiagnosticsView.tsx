import * as React from "react";
import { Clipboard, Download, Stethoscope } from "lucide-react";
import { callBackend } from "../backend";
import type { DiagnosticsSnapshot } from "../types";

export function DiagnosticsView({
  diagnostics,
  onRefresh,
  onMessage,
  onProgress,
}: {
  diagnostics: DiagnosticsSnapshot | null;
  onRefresh: () => void;
  onMessage: (message: string) => void;
  onProgress: (message: string) => void;
}) {
  const [logMessage, setLogMessage] = React.useState("");
  const logs = diagnostics?.logs ?? [];
  const logText = logs.length ? logs.join("\n") : "";
  const logLines = React.useMemo(() => logs.map(formatDiagnosticLogLine), [logs]);

  const collectDiagnostics = () => {
    setLogMessage("正在生成诊断快照");
    onProgress("正在生成诊断快照");
    callBackend<string>("collect_diagnostics")
      .then((message) => {
        setLogMessage(message);
        onMessage(message);
        onRefresh();
      })
      .catch((error) => {
        const message = `生成诊断快照失败：${String(error)}`;
        setLogMessage(message);
        onMessage(message);
      })
      .finally(() => onProgress(""));
  };

  const copyLogs = () => {
    if (!logText) {
      setLogMessage("暂无日志可复制");
      return;
    }
    navigator.clipboard.writeText(logText)
      .then(() => setLogMessage("日志已复制"))
      .catch((error) => setLogMessage(`复制失败：${String(error)}`));
  };

  const exportLogs = () => {
    if (!logText) {
      setLogMessage("暂无日志可导出");
      return;
    }
    const blob = new Blob([`${logText}\n`], { type: "application/jsonl;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `codex-pilot-diagnostic-${Date.now()}.jsonl`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
    setLogMessage("日志已导出");
  };

  return (
    <div className="diagnosticsLayout">
    <section className="panel">
      <div className="panelHeader">
        <div className="panelTitle">
          <Stethoscope size={16} />
          <h2>运行检查</h2>
        </div>
        <div className="buttonRow">
          <button className="primary" onClick={collectDiagnostics} type="button">
            <Stethoscope size={16} />
            生成诊断快照
          </button>
          <button className="secondary" onClick={copyLogs} type="button">
            <Clipboard size={16} />
            复制日志
          </button>
          <button className="secondary" onClick={exportLogs} type="button">
            <Download size={16} />
            导出日志
          </button>
        </div>
      </div>
      <div className="checkList">
        {(diagnostics?.checks ?? []).map((check) => (
          <div className="checkRow" key={check.name}>
            <span className={`pill ${check.status}`}>{check.status}</span>
            <div>
              <strong>{check.name}</strong>
              <p>{check.detail}</p>
            </div>
          </div>
        ))}
        {!diagnostics?.checks.length && <p className="bodyText">暂无诊断数据。</p>}
      </div>
      {logMessage && <p className="formMessage logMessage">{logMessage}</p>}
      <pre className="logBlock">
        {logLines.length ? logLines.join("\n") : "暂无日志"}
      </pre>
    </section>
    </div>
  );
}

function formatDiagnosticLogLine(line: string): string {
  const text = String(line || "");
  try {
    const parsed = JSON.parse(text) as { ts?: number | string };
    const prefix = formatDiagnosticTimestamp(parsed?.ts);
    return prefix ? `${prefix} ${text}` : text;
  } catch (_error) {
    return text;
  }
}

function formatDiagnosticTimestamp(value: unknown): string {
  const raw = typeof value === "string" || typeof value === "number" ? Number(value) : Number.NaN;
  if (!Number.isFinite(raw) || raw <= 0) return "";
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (part: number, size = 2) => String(part).padStart(size, "0");
  return [
    date.getFullYear(),
    "-",
    pad(date.getMonth() + 1),
    "-",
    pad(date.getDate()),
    " ",
    pad(date.getHours()),
    ":",
    pad(date.getMinutes()),
    ":",
    pad(date.getSeconds()),
    ".",
    pad(date.getMilliseconds(), 3),
  ].join("");
}
