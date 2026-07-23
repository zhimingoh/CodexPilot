---
title: 一键同步全部对话测试报告
module: dialog-sync
change_id: 001-one-click-dialog-sync
tags: [dialog-sync, provider, test]
created: 2026-07-13
updated: 2026-07-13
status: active
summary: >
  16 项变更相关单元测试与 8 项集成场景全部通过，一键同步、刷新恢复和底层数据保护均符合验收标准。
---

# 一键同步全部对话 测试报告

> 关联模块：[spec.md](../../spec.md)
> 关联 Change：[change.md](change.md)
> 测试日期：2026-07-13
> 测试环境：Windows NT 10.0.26200.0 / Node.js v24.17.0 / npm 11.13.0 / rustc 1.97.0 / Vite 6.4.2 / Codex 应用内浏览器

## 测试总结

| 指标 | 数值 |
|------|------|
| 单元测试总数 | 16 |
| 单元测试通过 | 16 |
| 单元测试失败 | 0 |
| 集成测试总数 | 8 |
| 集成测试通过 | 8 |
| 集成测试失败 | 0 |
| 总体通过率 | 100%（24/24） |

本轮补齐了审查指出的三个回归面：初始快照失败后的“重新检查”、窗口 focus/visibility 与手动刷新策略、同步结束后等待快照刷新再解除 busy。UI preview 还提供首次检查失败注入，浏览器验证了失败态可以恢复为可同步态。

## 单元测试详情

### ✅ 通过的测试

| 测试文件 | 测试用例 | 对应 TODO |
|----------|----------|-----------|
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | rollout 与 SQLite drift 合计为待同步数量 | TODO-C1 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 无快照时显示禁用的“检查中” | TODO-C1 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 已有旧快照但刷新中时仍显示禁用的“检查中” | TODO-C1、TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 检查失败时显示可点击的“重新检查” | TODO-C1、TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 有 drift 时启用“同步全部对话” | TODO-C1、TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 同步中禁用重复触发 | TODO-C1、TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 无 drift 时显示禁用的“已同步” | TODO-C1、TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 一键同步命令不含 target 参数 | TODO-S1、TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | mount、可见 focus/visibility 与手动操作触发刷新；隐藏或同步 busy 时不触发 | TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 后置快照刷新完成前同步周期不结束 | TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 成功同步与刷新完成后返回完整结果 | TODO-C2 |
| `apps/codex-pilot-manager/src/dialogSync.test.ts` | 同步命令失败后仍保留错误并刷新快照 | TODO-C2 |
| `apps/codex-pilot-manager/src/recycleBinSupport.test.ts` | 快照缺失时复用稳定空数组，避免 effect 渲染循环 | TODO-C2 |
| `crates/codex-pilot-data/src/provider_sync/mod.rs` | 默认路径读取配置 Provider，并更新 rollout、SQLite、global-state 与备份 | TODO-S1 |
| `crates/codex-pilot-data/src/provider_sync/mod.rs` | 运行锁存在时跳过同步且不并发写入 | TODO-S1 |
| `crates/codex-pilot-data/src/provider_sync/mod.rs` | 显式 target 内部兼容路径保持可用 | TODO-S1 |

执行证据：

```text
npm test
EXIT 0

cargo test -p codex-pilot-data provider_sync
running 3 tests
3 passed; 0 failed
```

### ❌ 失败的测试

无。

## 集成 / 场景验证详情

### 场景 1：普通用户入口不再选择 Provider
- **操作步骤**：打开 UI preview，进入“对话维护”，统计同步区的输入、选择、预览和确认控件。
- **期望结果**：只保留一个同步操作按钮和只读当前 Provider。
- **实际结果**：✅ 符合预期。
- **证据**：Provider selector=0、预览按钮=0、确认按钮=0、同步按钮=1。

### 场景 2：单击直接执行并进入已同步
- **操作步骤**：在有待同步记录的 preview 中单击一次“同步全部对话”。
- **期望结果**：不进入确认态；命令完成并刷新为禁用的“已同步”。
- **实际结果**：✅ 符合预期。
- **证据**：状态类从 `ready` 变为 `synced`，同步区始终只有一个按钮。

### 场景 3：检查失败后重新检查
- **操作步骤**：使用 `?dialogSyncFailOnce=1` 打开 preview，进入对话维护后点击“重新检查”。
- **期望结果**：首次失败可见，按钮可点击；重试后恢复“同步全部对话”。
- **实际结果**：✅ 符合预期。
- **证据**：失败态为 `syncStatusCard failed`，“重新检查”已启用，点击后恢复按钮数量为 1。

### 场景 4：Provider 切换后的刷新策略
- **操作步骤**：执行刷新策略与同步周期测试，覆盖 mount、focus、visibility、顶部/页面手动刷新、失败重试和同步 busy。
- **期望结果**：页面回到可见状态或用户手动刷新时重新获取当前 Provider 快照；隐藏或同步 busy 时不发起生命周期刷新，后置刷新保持独占。
- **实际结果**：✅ 符合预期。
- **证据**：刷新策略、旧快照 loading 禁用和进行中 Promise 复用断言通过；同步入口同时检查 busy 与 refresh ref；静默 5 秒父级轮询不递增局部刷新标记。

### 场景 5：执行时 Provider 与数据保护链
- **操作步骤**：运行默认 Provider、显式 target 和运行锁三项 Rust 测试。
- **期望结果**：无 target 使用执行时配置 Provider；显式 target 兼容；备份、锁和数据更新行为不变。
- **实际结果**：✅ 符合预期。
- **证据**：`provider_sync` 测试组 3/3 通过。

### 场景 6：桌面与长 Provider 布局
- **操作步骤**：在 1120×760 preview 中展示 `team-relay-production-ap-southeast-1` 并检查同步卡片。
- **期望结果**：状态、按钮和 Provider 文本不重叠、不横向溢出。
- **实际结果**：✅ 符合预期。
- **证据**：页面 overflow=false、同步按钮=1；截图见 `docs/images/readme-dialog-maintenance.png`。

### 场景 7：390px 窄窗口布局
- **操作步骤**：普通 Vite 页面使用 390×844 视口，打开对话维护失败态。
- **期望结果**：同步卡片和“重新检查”按钮完整位于视口内，无文本或页面横向溢出。
- **实际结果**：✅ 符合预期。
- **证据**：document/card/button overflow 均为 false，cardWithinViewport=true，retryEnabled=true。

### 场景 8：前端与 Tauri 构建契约
- **操作步骤**：执行测试、TypeScript/主题检查、Vite 生产构建和 Tauri Rust 包检查。
- **期望结果**：命令退出码为 0，无本 change 引入的编译或构建错误。
- **实际结果**：✅ 符合预期。
- **证据**：`npm test`、`npm run check`、`npm run vite:build`、`cargo check -p codex-pilot-manager` 均为 EXIT 0。

## AC 覆盖

| AC | 覆盖证据 | 结果 |
|----|----------|------|
| AC-1 无需选择目标 | 场景 1 | ✅ |
| AC-2 单击直接同步 | 场景 2 + 无 target 命令断言 | ✅ |
| AC-3 执行时读取当前 Provider | 场景 5 + 无 target 命令断言 | ✅ |
| AC-4 按钮状态明确 | 状态模型测试 + 场景 2、3 | ✅ |
| AC-5 Provider 切换后手动跟随 | 场景 4、5 | ✅ |
| AC-6 数据保护保持不变 | 场景 5 | ✅ |
| AC-7 失败可见且界面可恢复 | 同步失败周期测试 + 场景 3 | ✅ |
| AC-8 不引入自动同步 | 刷新策略测试 + 场景 1 | ✅ |

## 未覆盖的测试场景

- **真实 Codex 客户端分组显示**：未对用户真实 `~/.codex` 执行破坏性同步，也未启动真实客户端观察分组变化；数据行为由隔离 fixture、Rust 集成测试和 UI preview 覆盖。

## 遗留问题

- `cargo check -p codex-pilot-manager` 通过，但当前工作区其他启动兼容代码存在 4 个 unused/dead-code warning：`lib.rs` 两个未使用参数、`show_main_window` 未使用、`build_codex_command_preview` 未使用。它们不来自本 change，本测试阶段未修改。
