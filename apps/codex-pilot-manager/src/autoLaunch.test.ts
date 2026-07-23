import { resolveAutoLaunchAction, type AutoLaunchDecision } from "./autoLaunch.js";

function expectEqual(actual: AutoLaunchDecision, expected: AutoLaunchDecision, name: string) {
  const actualJson = JSON.stringify(actual);
  const expectedJson = JSON.stringify(expected);
  if (actualJson !== expectedJson) {
    throw new Error(`${name}\nexpected: ${expectedJson}\nactual:   ${actualJson}`);
  }
}

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: false,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  { kind: "skip", markAttempted: false },
  "does not auto launch when the preference is off",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  {
    kind: "run",
    markAttempted: true,
    command: "launch_codex",
    progress: "正在自动启动 ChatGPT",
    message: "正在自动启动 ChatGPT",
  },
  "launches the resolved desktop host when auto launch is enabled and state is safe",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "reinject",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  {
    kind: "run",
    markAttempted: true,
    command: "reinject_codex",
    progress: "正在自动注入 CodexPilot",
    message: "正在自动注入 CodexPilot",
  },
  "automatically injects into an already running host when state is safe",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "restart",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  {
    kind: "stop",
    markAttempted: true,
    message: "ChatGPT 已运行但没有调试端口，需要手动确认重启并注入",
  },
  "does not restart an unrelated running host",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: true,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  { kind: "skip", markAttempted: false },
  "does not run more than once per page load",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: true,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  { kind: "skip", markAttempted: false },
  "does not start a second launch while launch is in progress",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: true,
    launching: false,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  { kind: "skip", markAttempted: false },
  "does not retry automatically after one failed auto action",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launch",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: false,
    hostLabel: "ChatGPT",
  }),
  {
    kind: "stop",
    markAttempted: true,
    message: "未找到 ChatGPT 安装或启动路径不可用，已跳过自动启动/注入",
  },
  "does not auto launch when the desktop host is not installed",
);

expectEqual(
  resolveAutoLaunchAction({
    actionKind: "launching",
    autoLaunchOnOpen: true,
    alreadyAttempted: false,
    alreadyFailed: false,
    launching: false,
    codexInstalled: true,
    hostLabel: "ChatGPT",
  }),
  { kind: "skip", markAttempted: false },
  "skips without marking attempted while backend is launching",
);
