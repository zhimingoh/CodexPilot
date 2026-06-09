import type { UpdateSnapshot } from "./types.js";

export type UpdateReminderView = {
  title: string;
  versionText: string;
  detail: string;
  canOpenRelease: boolean;
  canIgnore: boolean;
  hasAttention: boolean;
};

export function currentVersionLabel(snapshot: UpdateSnapshot | null, appVersion: string | null): string {
  const version = snapshot?.currentVersion ?? appVersion;
  return version && version !== "未知" ? `v${version}` : "未知";
}

export function updateReminderView(
  snapshot: UpdateSnapshot | null,
  appVersion: string | null,
): UpdateReminderView {
  if (!snapshot) {
    return {
      title: "当前版本",
      versionText: currentVersionLabel(null, appVersion),
      detail: "正在检查更新",
      canOpenRelease: false,
      canIgnore: false,
      hasAttention: false,
    };
  }

  const latestLabel = snapshot.latestVersion ? `v${snapshot.latestVersion}` : null;

  if (snapshot.status === "available") {
    return {
      title: "当前版本",
      versionText: currentVersionLabel(snapshot, appVersion),
      detail: latestLabel ? `发现新版本 ${latestLabel}` : "发现新版本",
      canOpenRelease: Boolean(snapshot.releaseUrl),
      canIgnore: Boolean(snapshot.latestTag),
      hasAttention: true,
    };
  }

  if (snapshot.status === "ignored") {
    return {
      title: "当前版本",
      versionText: currentVersionLabel(snapshot, appVersion),
      detail: latestLabel ? `已忽略 ${latestLabel}` : "已忽略此版本",
      canOpenRelease: Boolean(snapshot.releaseUrl),
      canIgnore: false,
      hasAttention: false,
    };
  }

  if (snapshot.status === "failed") {
    return {
      title: "当前版本",
      versionText: currentVersionLabel(snapshot, appVersion),
      detail: "暂时无法检查更新",
      canOpenRelease: Boolean(snapshot.releaseUrl),
      canIgnore: false,
      hasAttention: false,
    };
  }

  if (snapshot.status === "checking") {
    return {
      title: "当前版本",
      versionText: currentVersionLabel(snapshot, appVersion),
      detail: "正在检查更新",
      canOpenRelease: Boolean(snapshot.releaseUrl),
      canIgnore: false,
      hasAttention: false,
    };
  }

  return {
    title: "当前版本",
    versionText: currentVersionLabel(snapshot, appVersion),
    detail: "已是最新版本",
    canOpenRelease: Boolean(snapshot.releaseUrl),
    canIgnore: false,
    hasAttention: false,
  };
}
