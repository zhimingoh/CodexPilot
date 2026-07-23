---
title: 一键同步全部对话实施偏差记录
module: dialog-sync
change_id: 001-one-click-dialog-sync
created: 2026-07-13
updated: 2026-07-13
---

# 一键同步全部对话实施偏差记录

## 窄窗口布局修复

- **发现**：按 TODO-C3 在 760px 窗口验证时，现有全局响应式规则把 shell 改为单列，但 sidebar 仍保持 `100vh` 高度，导致包括对话同步卡在内的全部页面主体被固定预览画布裁掉。
- **偏差**：在 change 预估文件之外修改 `apps/codex-pilot-manager/src/styles.css`。
- **处理**：窄窗口下把 shell 定义为“自动高度导航行 + 可滚动内容行”，并取消 sidebar 的视口高度；不改变桌面布局和对话同步业务行为。
- **验证**：在 760px 视口重新检查页面宽高、同步按钮边界和完整截图。
