import type { ProviderSyncSnapshot } from "./types.js";

export type DialogSyncAction = {
  kind: "checking" | "failed" | "ready" | "syncing" | "synced";
  label: string;
  disabled: boolean;
};

export type DialogSyncLoadState = "loading" | "ready" | "failed";
export type DialogSyncRefreshTrigger = "mount" | "focus" | "visibility" | "manual";

export function dialogSyncPending(snapshot: ProviderSyncSnapshot) {
  return snapshot.rolloutRewriteNeeded + snapshot.sqliteProviderRowsNeedingSync;
}

export function dialogSyncAction(
  snapshot: ProviderSyncSnapshot | null,
  syncing: boolean,
  loadState: DialogSyncLoadState = snapshot ? "ready" : "loading",
): DialogSyncAction {
  if (syncing) {
    return { kind: "syncing", label: "同步中", disabled: true };
  }
  if (loadState === "loading") {
    return { kind: "checking", label: "检查中", disabled: true };
  }
  if (loadState === "failed") {
    return { kind: "failed", label: "重新检查", disabled: false };
  }
  if (!snapshot) {
    return { kind: "checking", label: "检查中", disabled: true };
  }
  if (dialogSyncPending(snapshot) <= 0) {
    return { kind: "synced", label: "已同步", disabled: true };
  }
  return { kind: "ready", label: "同步全部对话", disabled: false };
}

export function currentProviderSyncCommand() {
  return { command: "sync_provider_sessions", refreshScope: "dialog-sync" } as const;
}

export function shouldRefreshDialogSync(
  trigger: DialogSyncRefreshTrigger,
  visibilityState: DocumentVisibilityState,
  syncing = false,
) {
  if (syncing) return false;
  return trigger === "mount" || trigger === "manual" || visibilityState === "visible";
}

export async function runDialogSyncCycle<T>(
  runSync: () => Promise<T>,
  refreshSync: () => Promise<unknown>,
) {
  let syncResult: T | null = null;
  let syncError: unknown = null;
  let refreshError: unknown = null;

  try {
    syncResult = await runSync();
  } catch (error) {
    syncError = error;
  }

  try {
    await refreshSync();
  } catch (error) {
    refreshError = error;
  }

  return { syncResult, syncError, refreshError };
}
