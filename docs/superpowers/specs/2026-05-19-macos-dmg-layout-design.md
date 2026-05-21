# macOS DMG 安装布局设计

## 背景

早期 macOS 打包脚本只把 `CodexPilot.app` 放进 DMG。用户打开后看到的是普通 Finder 窗口，缺少标准 macOS 应用常见的拖拽安装体验。

当前实现以 `scripts/package-macos.sh` 为准：使用 `create-dmg` 生成可视化安装盘布局，左侧是 `CodexPilot.app`，右侧是 Applications drop link，中间背景箭头提示拖拽安装。安装盘还包含一个 `已损坏修复.command` 辅助脚本，用于处理未签名应用在 macOS 上可能出现的“已损坏”提示。

## 目标

- DMG 中包含 `CodexPilot.app`。
- DMG 中包含指向 `/Applications` 的快捷方式。
- DMG 中包含 `已损坏修复.command`。
- 打开 DMG 时 Finder 以图标视图展示。
- 图标尺寸放大，应用和 Applications 图标位置固定。
- 使用背景图展示品牌标题和拖拽方向。
- 输出文件名仍保持 `CodexPilot-<version>-macos-<arch>.dmg`。

## 非目标

- 不照搬 Discord 的品牌图形、颜色、logo 或文案。
- 不加入签名和公证流程。
- 不改变 Tauri app 构建方式。
- 不把 `已损坏修复.command` 作为长期替代签名/公证的正式方案。它只是未签名分发阶段的辅助入口。

## 打包流程

脚本继续构建 launcher sidecar、Manager 前端和 Tauri app。DMG 生成阶段调整为：

1. 创建 staging 目录。
2. 复制 `CodexPilot.app` 到 staging。
3. 写入 `已损坏修复.command`，用于执行 quarantine 清理和重新打开提示。
4. 如果修复脚本图标生成器存在，为 `已损坏修复.command` 写入自定义图标。
5. 生成 `.background/background.svg`，并尽量用 `sips` 转成 `.background/background.png`。
6. 调用 `create-dmg`：
   - 设置卷名、窗口大小和背景图。
   - 放置 `CodexPilot.app` 图标。
   - 使用 `--app-drop-link` 放置 Applications 入口。
   - 放置 `已损坏修复.command` 图标。
   - 隐藏 `.app` 和 `.command` 扩展名。
7. 清理 staging 目录。

## 背景图

背景图使用项目脚本生成，不提交二进制图片。视觉要求：

- 浅色背景。
- 顶部显示安装提示文案。
- 中间只保留一个浅灰色 Lucide `arrow-right` 线性箭头作为方向提示，不使用白色背景块，避免看起来像第三个可拖拽图标。
- 保持克制，不做营销页。

当前脚本优先使用 macOS 自带 `sips` 把 SVG 背景转成 PNG；如果转换失败，则继续把脚本生成的 SVG 背景交给 `create-dmg`。这保持了当前满意的安装盘视觉效果，也避免因为 PNG 转换失败而丢失背景。

## 风险与处理

- `create-dmg` 是当前实现依赖：打包环境需要预先安装该工具。
- `create-dmg` 底层仍依赖 macOS DMG/Finder 能力：无 GUI 或受限环境可能失败，脚本应给出明确错误。
- DMG 创建在当前沙箱下可能报“设备未配置”：仍需要提升权限运行打包脚本。
- 背景图 PNG 转换依赖 macOS 自带工具；转换失败时继续使用 SVG 背景，不阻塞基础 DMG 输出。
- `已损坏修复.command` 会降低未签名阶段的使用门槛，但不能替代正式签名和公证。

## 验收

- DMG 能正常生成。
- 挂载后包含 `CodexPilot.app`、Applications 入口和 `已损坏修复.command`。
- Finder 打开后是拖拽安装布局。
- `scripts/package-macos.sh` 仍支持 `TARGET_TRIPLE`。
