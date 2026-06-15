import * as React from "react";
import { Network, Stethoscope, Terminal, Trash2 } from "lucide-react";
import { Metric } from "../components/primitives";
import type {
  BackendStatus,
  DiagnosticsSnapshot,
  LaunchSnapshot,
  ProviderSnapshot,
  RecycleBinSnapshot,
  ViewId,
} from "../types";

function canRunLaunchAction(launch: LaunchSnapshot | null): boolean {
  if (!launch) return false;
  return ["launch", "reinject", "restart", "running"].includes(launch.actionKind);
}

function providerModeLabel(provider: ProviderSnapshot | null): string {
  if (!provider) return "检查中";
  if (provider.externalProvider) return "纯API态（外部管理）";
  const mode = provider.mode;
  if (mode === "hybrid") return "中转态";
  if (mode === "api") return "纯API态";
  if (mode === "official") return "登录态";
  if (provider.chatgptAuthenticated) return "登录态（推断）";
  return "未知";
}

function backendStatusLabel(status: BackendStatus | null): string {
  if (!status) return "未连接";
  if (status.status === "running") return "已连接";
  return status.status || "未连接";
}

export function OverviewView({
  status,
  appVersion,
  launch,
  recycleBin,
  diagnostics,
  provider,
  onNavigate,
}: {
  status: BackendStatus | null;
  appVersion: string | null;
  launch: LaunchSnapshot | null;
  recycleBin: RecycleBinSnapshot | null;
  diagnostics: DiagnosticsSnapshot | null;
  provider: ProviderSnapshot | null;
  onNavigate: (view: ViewId) => void;
}) {
  const deletedCount = recycleBin?.entries.length ?? 0;
  const recoverableCount = recycleBin?.entries.filter((entry) => entry.recoverable).length ?? 0;
  const diagnosticsChecks = diagnostics?.checks ?? [];
  const failingChecks = diagnosticsChecks.filter((check) => !["ok", "pass", "passed"].includes(check.status)).length;
  const backendState = backendStatusLabel(status);
  const displayVersion = appVersion ?? status?.version ?? "未知";

  return (
    <div className="taskStack">
      <section className="taskPanel primaryTask overviewLaunchTask">
        <div className="taskHeader">
          <div>
            <div className="panelTitle compactTitle titleLine">
              <span className="titleIcon">
                <Terminal size={16} />
              </span>
              <h2>启动与注入</h2>
              <span className={`statusPill ${canRunLaunchAction(launch) ? "ok" : "warning"}`}>
                <span className={`statusDot ${canRunLaunchAction(launch) ? "ok" : "warning"}`} />
                {launch?.actionLabel ?? "检查中"}
              </span>
            </div>
            <p className="taskSummary">主面板集中展示启动前最关键的状态和端口，详细路径与命令预览保留在启动设置页。</p>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="后端" value={backendState} />
          <Metric label="Codex 应用" value={launch?.appPath ? "已发现" : "未发现"} />
          <Metric label="调试端口" value={String(launch?.debugPort ?? "-")} />
          <Metric label="连接端口" value={String(launch?.helperPort ?? "-")} />
          <Metric label="版本" value={displayVersion} />
        </dl>
        <div className="taskFooter">
          <span className={`statusDot ${canRunLaunchAction(launch) ? "ok" : "warning"}`} />
          <span>{launch?.detail ?? "需要检查 Codex 应用路径或启动偏好"}</span>
          <button className="linkButton" onClick={() => onNavigate("launch")} type="button">查看启动设置</button>
        </div>
      </section>

      <section className="taskPanel summaryTask">
        <div className="taskHeader summaryTaskHeader">
          <div className="panelTitle compactTitle">
            <span className="rowIcon"><Trash2 size={14} /></span>
            <h2>对话维护</h2>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="已删除" value={`${deletedCount} 条`} />
          <Metric label="可恢复" value={`${recoverableCount} 条`} />
        </dl>
        <button className="secondary summaryAction" onClick={() => onNavigate("sessions")} type="button">打开对话维护</button>
      </section>

      <section className="taskPanel summaryTask">
        <div className="taskHeader summaryTaskHeader">
          <div className="panelTitle compactTitle">
            <span className="rowIcon"><Stethoscope size={14} /></span>
            <h2>诊断摘要</h2>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="检查项" value={`${diagnosticsChecks.length} 项`} />
          <Metric label="需关注" value={`${failingChecks} 项`} />
        </dl>
        <button className="secondary summaryAction" onClick={() => onNavigate("diagnostics")} type="button">查看诊断</button>
      </section>

      <section className="taskPanel summaryTask">
        <div className="taskHeader summaryTaskHeader">
          <div className="panelTitle compactTitle">
            <span className="rowIcon"><Network size={14} /></span>
            <h2>模型通道</h2>
          </div>
        </div>
        <dl className="metricGrid overviewMetrics">
          <Metric label="当前模式" value={providerModeLabel(provider)} />
          <Metric label="ChatGPT 登录" value={provider?.chatgptAuthenticated ? "已登录" : "未登录"} />
        </dl>
        <button className="secondary summaryAction" onClick={() => onNavigate("provider")} type="button">管理模型通道</button>
      </section>
    </div>
  );
}
