export type AutoLaunchInput = {
  actionKind: string;
  autoLaunchOnOpen: boolean;
  alreadyAttempted: boolean;
  launching: boolean;
};

export type AutoLaunchDecision =
  | { kind: "skip"; markAttempted: false; message?: undefined; command?: undefined; progress?: undefined }
  | { kind: "stop"; markAttempted: true; message?: string; command?: undefined; progress?: undefined }
  | { kind: "run"; markAttempted: true; command: "launch_codex"; progress: string; message: string };

export function resolveAutoLaunchAction(input: AutoLaunchInput): AutoLaunchDecision {
  if (input.launching || input.alreadyAttempted || !input.autoLaunchOnOpen) {
    return { kind: "skip", markAttempted: false };
  }

  if (input.actionKind === "launch") {
    const progress = "正在自动启动 Codex";
    return {
      kind: "run",
      markAttempted: true,
      command: "launch_codex",
      progress,
      message: progress,
    };
  }

  if (input.actionKind === "reinject") {
    return {
      kind: "stop",
      markAttempted: true,
      message: "Codex 已运行，已跳过自动注入；需要时可手动重新注入",
    };
  }

  if (input.actionKind === "restart") {
    return {
      kind: "stop",
      markAttempted: true,
      message: "Codex 已运行但没有调试端口，需要手动确认重启并注入",
    };
  }

  return { kind: "stop", markAttempted: true };
}
