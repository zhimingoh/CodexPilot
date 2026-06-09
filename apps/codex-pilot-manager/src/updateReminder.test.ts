import { updateReminderView } from "./updateReminder.js";
import type { UpdateSnapshot } from "./types.js";

function snapshot(status: UpdateSnapshot["status"]): UpdateSnapshot {
  return {
    currentVersion: "1.3.2",
    latestVersion: "1.3.3",
    latestTag: "v1.3.3",
    releaseUrl: "https://github.com/hl9565/CodexPilot/releases/tag/v1.3.3",
    releaseName: "CodexPilot v1.3.3",
    publishedAt: "2026-06-09T00:00:00Z",
    status,
    error: status === "failed" ? "network failed" : null,
  };
}

function expectView(
  actual: ReturnType<typeof updateReminderView>,
  expected: Partial<ReturnType<typeof updateReminderView>>,
  name: string,
) {
  for (const [key, value] of Object.entries(expected)) {
    const actualValue = actual[key as keyof typeof actual];
    if (actualValue !== value) {
      throw new Error(`${name}: expected ${key}=${String(value)}, actual ${String(actualValue)}`);
    }
  }
}

expectView(
  updateReminderView(null, "1.3.2"),
  {
    versionText: "v1.3.2",
    detail: "正在检查更新",
    hasAttention: false,
  },
  "unknown state shows checking copy",
);

expectView(
  updateReminderView(null, null),
  {
    versionText: "未知",
    detail: "正在检查更新",
  },
  "unknown version does not get a v prefix",
);

expectView(
  updateReminderView(snapshot("available"), null),
  {
    detail: "发现新版本 v1.3.3",
    canOpenRelease: true,
    canIgnore: true,
    hasAttention: true,
  },
  "available update enables release and ignore actions",
);

expectView(
  updateReminderView(snapshot("latest"), null),
  {
    detail: "已是最新版本",
    canOpenRelease: true,
    canIgnore: false,
    hasAttention: false,
  },
  "latest state is quiet",
);

expectView(
  updateReminderView(snapshot("ignored"), null),
  {
    detail: "已忽略 v1.3.3",
    canOpenRelease: true,
    canIgnore: false,
    hasAttention: false,
  },
  "ignored state clears attention",
);

expectView(
  updateReminderView(snapshot("failed"), null),
  {
    detail: "暂时无法检查更新",
    canIgnore: false,
    hasAttention: false,
  },
  "failed state stays quiet",
);
