import * as React from "react";
import { CheckCircle2, ExternalLink, RefreshCw, RotateCw, Tag, XCircle } from "lucide-react";
import { updateReminderView } from "./updateReminder";
import type { UpdateSnapshot } from "./types";

export function UpdateReminderButton({
  appVersion,
  snapshot,
  checking,
  onCheck,
  onIgnore,
  onOpenRelease,
}: {
  appVersion: string | null;
  snapshot: UpdateSnapshot | null;
  checking: boolean;
  onCheck: () => void;
  onIgnore: (tag: string) => void;
  onOpenRelease: (url: string) => void;
}) {
  const [open, setOpen] = React.useState(false);
  const view = updateReminderView(snapshot, appVersion);
  const releaseUrl = snapshot?.releaseUrl ?? null;
  const latestTag = snapshot?.latestTag ?? null;
  const isFailed = snapshot?.status === "failed";
  const isLatest = snapshot?.status === "latest";

  React.useEffect(() => {
    if (!open) return;
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (!target.closest(".updateReminder")) {
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", handlePointerDown);
    return () => document.removeEventListener("pointerdown", handlePointerDown);
  }, [open]);

  return (
    <div className="updateReminder">
      <button
        className={`secondary iconButton updateButton ${view.hasAttention ? "attention" : ""}`}
        onClick={() => setOpen((current) => !current)}
        title={view.hasAttention ? "发现新版本" : "检查更新"}
        type="button"
      >
        {checking ? <RotateCw className="spinIcon" size={16} /> : <Tag size={16} />}
        {view.hasAttention && <span className="attentionDot" />}
      </button>
      {open && (
        <section className="updatePopover" aria-label="版本更新">
          <div className="updateCardTop">
            <span>{view.title}</span>
            <button
              className="secondary iconButton miniIconButton"
              disabled={checking}
              onClick={onCheck}
              title="重新检查"
              type="button"
            >
              <RefreshCw size={14} />
            </button>
          </div>
          <div className="updateVersionRow">
            <strong>{view.versionText}</strong>
            {isLatest && (
              <span className="updateOkIcon" aria-label="已是最新版本">
                <CheckCircle2 size={16} />
              </span>
            )}
            {isFailed && (
              <span className="updateFailIcon" aria-label="检查失败">
                <XCircle size={16} />
              </span>
            )}
          </div>
          <p className="updateDetail">{view.detail}</p>
          {snapshot?.releaseName && <p className="updateReleaseName">{snapshot.releaseName}</p>}
          {snapshot?.error && <p className="updateError">{snapshot.error}</p>}
          <div className="updateActions">
            <button className="secondary" disabled={checking} onClick={onCheck} type="button">
              <RefreshCw size={14} />
              重新检查
            </button>
            <button
              className="secondary"
              disabled={!view.canOpenRelease || !releaseUrl}
              onClick={() => releaseUrl && onOpenRelease(releaseUrl)}
              type="button"
            >
              <ExternalLink size={14} />
              查看发布
            </button>
            {view.canIgnore && latestTag && (
              <button className="secondary" onClick={() => onIgnore(latestTag)} type="button">
                忽略此版本
              </button>
            )}
          </div>
        </section>
      )}
    </div>
  );
}
