import {
  currentProviderSyncCommand,
  dialogSyncAction,
  dialogSyncPending,
  runDialogSyncCycle,
  shouldRefreshDialogSync,
  type DialogSyncAction,
} from "./dialogSync.js";
import type { ProviderSyncSnapshot } from "./types.js";

function snapshot(
  rolloutRewriteNeeded: number,
  sqliteProviderRowsNeedingSync: number,
): ProviderSyncSnapshot {
  return {
    targetProvider: "current-provider",
    currentProvider: "current-provider",
    availableProviders: ["current-provider", "old-provider"],
    rolloutFiles: 10,
    rolloutRewriteNeeded,
    sqliteRows: 12,
    sqliteProviderRowsNeedingSync,
    sqliteTotalUpdatesNeeded: rolloutRewriteNeeded + sqliteProviderRowsNeedingSync,
    rolloutProviders: [],
    sqliteProviders: [],
  };
}

function expectEqual<T>(actual: T, expected: T, name: string) {
  const actualJson = JSON.stringify(actual);
  const expectedJson = JSON.stringify(expected);
  if (actualJson !== expectedJson) {
    throw new Error(`${name}\nexpected: ${expectedJson}\nactual:   ${actualJson}`);
  }
}

expectEqual(dialogSyncPending(snapshot(3, 4)), 7, "adds rollout and SQLite drift");

expectEqual<DialogSyncAction>(
  dialogSyncAction(null, false, "loading"),
  { kind: "checking", label: "检查中", disabled: true },
  "missing snapshot is checking",
);

expectEqual<DialogSyncAction>(
  dialogSyncAction(snapshot(1, 0), false, "loading"),
  { kind: "checking", label: "检查中", disabled: true },
  "refreshing an existing snapshot disables stale sync actions",
);

expectEqual<DialogSyncAction>(
  dialogSyncAction(null, false, "failed"),
  { kind: "failed", label: "重新检查", disabled: false },
  "failed snapshot check can be retried",
);

expectEqual<DialogSyncAction>(
  dialogSyncAction(snapshot(1, 0), false),
  { kind: "ready", label: "同步全部对话", disabled: false },
  "provider drift enables one-click sync",
);

expectEqual<DialogSyncAction>(
  dialogSyncAction(snapshot(1, 0), true),
  { kind: "syncing", label: "同步中", disabled: true },
  "running sync cannot be triggered twice",
);

expectEqual<DialogSyncAction>(
  dialogSyncAction(snapshot(0, 0), false),
  { kind: "synced", label: "已同步", disabled: true },
  "aligned history is already synced",
);

expectEqual(
  currentProviderSyncCommand(),
  { command: "sync_provider_sessions", refreshScope: "dialog-sync" },
  "one-click sync has no target arguments or message-clobbering manager refresh",
);

expectEqual(
  [
    shouldRefreshDialogSync("mount", "hidden"),
    shouldRefreshDialogSync("focus", "visible"),
    shouldRefreshDialogSync("focus", "hidden"),
    shouldRefreshDialogSync("visibility", "visible"),
    shouldRefreshDialogSync("visibility", "hidden"),
    shouldRefreshDialogSync("manual", "hidden"),
    shouldRefreshDialogSync("focus", "visible", true),
    shouldRefreshDialogSync("manual", "visible", true),
  ],
  [true, true, false, true, false, true, false, false],
  "refresh policy covers lifecycle/manual triggers without racing an active sync",
);

let releaseRefresh!: () => void;
const refreshGate = new Promise<void>((resolve) => {
  releaseRefresh = resolve;
});
let cycleSettled = false;
const cycle = runDialogSyncCycle(
  async () => "同步完成",
  async () => refreshGate,
).then((result) => {
  cycleSettled = true;
  return result;
});
await Promise.resolve();
await Promise.resolve();
expectEqual(cycleSettled, false, "sync cycle stays busy while snapshot refresh is pending");
releaseRefresh();
expectEqual(
  await cycle,
  { syncResult: "同步完成", syncError: null, refreshError: null },
  "sync cycle settles after snapshot refresh",
);

let refreshedAfterSyncFailure = false;
const failedCycle = await runDialogSyncCycle(
  async () => {
    throw new Error("同步命令失败");
  },
  async () => {
    refreshedAfterSyncFailure = true;
  },
);
expectEqual(
  [failedCycle.syncResult, String(failedCycle.syncError), failedCycle.refreshError, refreshedAfterSyncFailure],
  [null, "Error: 同步命令失败", null, true],
  "sync failure remains visible and still refreshes the snapshot",
);
