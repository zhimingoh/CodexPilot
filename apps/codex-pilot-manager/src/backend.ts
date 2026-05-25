import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { mockBackend } from "./dev/mockSnapshots";

const mode = (import.meta as ImportMeta & { env?: { MODE?: string } }).env?.MODE;
export const isUiPreviewMode = mode === "ui-preview";

function formatManagerError(err: unknown): string {
  if (err && typeof err === "object" && "kind" in err && "detail" in err) {
    const { kind, detail } = err as { kind: string; detail: string };
    switch (kind) {
      case "InvalidInput":
        return `[输入错误] ${detail}`;
      case "NotFound":
        return `[未找到] ${detail}`;
      case "Conflict":
        return `[冲突] ${detail}`;
      case "Io":
        return `[IO 错误] ${detail}（可查看诊断页）`;
      case "Internal":
        return `[系统错误] ${detail}（可查看诊断页）`;
      default:
        return detail || String(err);
    }
  }
  return typeof err === "string" ? err : String(err);
}

export function callBackend<T>(command: string, args?: unknown): Promise<T> {
  if (isUiPreviewMode) {
    return mockBackend<T>(command, args);
  }
  return invoke<T>(command, args as InvokeArgs | undefined).catch((err) => {
    throw formatManagerError(err);
  });
}
