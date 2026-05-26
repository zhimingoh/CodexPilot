# provider.rs 副作用 / 事务性调研报告

目标函数：

- `apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:224-261` `apply_provider`
- `apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:264-302` `save_provider_profile`

本报告只基于当前真实代码梳理副作用、失败模式和事务性方案，不包含任何实现改动。

## Section 1：副作用步骤拆解

说明：

- 只列“有副作用”或“会触发下层副作用”的步骤；纯内存判断、字符串拼接、枚举分支不算副作用。
- 行号同时标注入口函数和真正发生副作用的下层 helper / core 实现，便于维护者复核。

### `apply_provider` 副作用步骤

| 步骤 | 行号范围 | 副作用类型 | 一句话描述 | 是否可回滚 + 回滚方式 |
| --- | --- | --- | --- | --- |
| 1 | `provider.rs` 225-228；`provider_store.rs` 47-59 | 读文件 / 调下层模块 | 读取 `provider-profiles.json` 并解析当前 profiles / snapshot 状态，决定实际要应用哪个 profile | 不涉及回滚；只读 |
| 2 | `provider.rs` 230-239；`relay_config.rs` 131-159, 210-247；`relay_config_toml.rs` 11-22, 25-62 | 写文件 / 调 `codex_pilot_core::relay_config::*` | `requested_mode = HybridApi` 时，要求登录态存在，然后给 `~/.codex/config.toml` 写备份 `.bak`，再把中转配置写入 `config.toml` | 部分可回滚；理论上可用刚生成的 `.bak` 或已有官方 snapshot 回写 `config.toml`，但当前函数没自动回滚 |
| 3 | `provider.rs` 230-247；`relay_config.rs` 169-195, 263-300；`relay_config_toml.rs` 11-22, 64-100, 103-110 | 写文件 / 调 `codex_pilot_core::relay_config::*` | `requested_mode = Api` 时，先把 `OPENAI_API_KEY` 写入 `~/.codex/auth.json`，再给 `~/.codex/config.toml` 写备份 `.bak` 并写入 API 中转配置 | 部分可回滚；`config.toml` 可用 `.bak` / snapshot 回退，`auth.json` 只能靠重新写旧内容或删除 `OPENAI_API_KEY` 回退 |
| 4 | `provider.rs` 255-257；`provider_store.rs` 180-217；`relay_config.rs` 326-358；`relay_config_toml.rs` 11-22, 111-122 | 写文件 / 调 `codex_pilot_core::relay_config::*` | `requested_mode = None` 且 profile 的 `authenticated_behavior = OfficialDirect`、本机有登录态、并且存在官方 snapshot 时，用 snapshot 覆盖 `~/.codex/config.toml`，随后清理 `auth.json` 里的 `OPENAI_API_KEY` | 部分可回滚；可用 restore 前新生成的 `.bak` 回退 `config.toml`，`auth.json` 被清 key 后只能靠旧备份或重新写回 |
| 5 | `provider.rs` 255-257；`provider_store.rs` 180-223；`relay_config.rs` 131-159, 169-195, 210-300；`relay_config_toml.rs` 11-22, 25-110 | 写文件 / 调 `codex_pilot_core::relay_config::*` | `requested_mode = None` 但进入退化分支或普通中转分支时，最终仍调用 `apply_relay_provider_config_with_protocol` 或 `apply_api_provider_config_with_protocol`，落点与步骤 2/3 相同 | 部分可回滚；同步骤 2/3 |

### `save_provider_profile` 副作用步骤

| 步骤 | 行号范围 | 副作用类型 | 一句话描述 | 是否可回滚 + 回滚方式 |
| --- | --- | --- | --- | --- |
| 1 | `provider.rs` 268；`provider_store.rs` 47-59 | 读文件 / 调下层模块 | 读取现有 `provider-profiles.json` 为 `state` | 不涉及回滚；只读 |
| 2 | `provider.rs` 290；`provider_store.rs` 148-162；`relay_config.rs` 311-324 | 读文件 / 调下层模块 | 如果 `state.official_config_snapshot` 为空，尝试从当前 `~/.codex/config.toml` 捕获官方 snapshot 到内存 state | 可回滚但通常无需回滚；这一步只改内存，放弃本次保存即可 |
| 3 | `provider.rs` 291-292；`provider_store.rs` 16-18, 62-73 | 写文件 / 调下层模块 | 把更新后的 profiles、active profile 和可能新增的 official snapshot 写回 `app_state_dir/provider-profiles.json` | 可回滚；可用保存前的原文件内容 restore，或删掉新文件后重建旧状态 |
| 4 | `provider.rs` 293-294；`provider_store.rs` 165-177, 180-243 | 调下层模块 / 触发后续写文件 | 如果当前保存的 profile 变成 active profile，则继续调用 `apply_active_profile`，把内存 state 对应的 profile 应用到 Codex 配置 | 部分可回滚；要结合 `apply_profile_now` 实际落地分支回退 |
| 5 | `provider.rs` 293-294；`provider_store.rs` 184-199, 235-242；`relay_config.rs` 131-159, 169-195, 210-300；`relay_config_toml.rs` 11-22, 25-110 | 写文件 / 调 `codex_pilot_core::relay_config::*` | `apply_active_profile -> apply_profile_now` 的普通/退化中转分支：写 `auth.json`（API 分支）和/或备份并改写 `config.toml` | 部分可回滚；方式同 `apply_provider` 的步骤 2/3/5 |
| 6 | `provider.rs` 293-294；`provider_store.rs` 201-217；`relay_config.rs` 326-358；`relay_config_toml.rs` 11-22, 111-122 | 写文件 / 调 `codex_pilot_core::relay_config::*` | `apply_active_profile -> apply_profile_now` 的官方直连恢复分支：用 snapshot 覆盖 `config.toml`，并尝试清掉 `auth.json` 中的 `OPENAI_API_KEY` | 部分可回滚；方式同 `apply_provider` 的步骤 4 |

## Section 2：失败模式分析

### `apply_provider` 失败模式

| 步骤 | 失败后会留下什么不一致状态 | 重试是否安全 | 不安全 / 部分安全时的具体后果 |
| --- | --- | --- | --- |
| 1 | 无持久化改动；只是无法读取 profiles，函数直接失败 | 安全 | 无 |
| 2 | 可能已生成新的 `config.toml.codex-pilot-backup-*.bak`，也可能 `config.toml` 已被新中转配置覆盖；profiles 状态不变 | 部分安全 | 重试通常会继续覆盖 `config.toml` 并再生成一份 `.bak`，状态最终可收敛，但会累积备份；如果第一次已写成功而调用方超时重试，会重复制造备份并覆盖人工临时修改 |
| 3 | 最棘手：可能 `auth.json` 已写入新 `OPENAI_API_KEY`，但 `config.toml` 还没改成功；也可能 `config.toml` 已改、`auth.json` 已换新 key，而调用方只看到失败 | 部分安全 | 重试会再次覆盖 `auth.json`、继续生成 `.bak` 并重写 `config.toml`。最终多数情况下能收敛，但失败中途会留下“API key 已换、新 config 未生效”或“中转已生效但调用方以为失败”的半生效状态 |
| 4 | `config.toml` 可能已经恢复到 snapshot，但 `auth.json` 清 key 是 best effort 且错误被吞掉，因此可能出现“官方 config 已恢复，但 auth.json 里还残留旧 `OPENAI_API_KEY`” | 部分安全 | 重试通常安全，因为 snapshot 是确定值；但每次重试都会再生成一份 `.bak`。残留 key 本身不一定立即破坏官方直连，但会造成状态判断和人工排障混乱 |
| 5 | 与步骤 2/3 类似，但更隐蔽，因为这是 `apply_profile_now` 的内部退化分支，调用方看到的是“按 profile 应用失败/成功”，不是显式的 mode 应用 | 部分安全 | 后果同步骤 2/3：重复备份、`auth.json` 与 `config.toml` 半生效、消息与真实落地状态可能错位 |

### `save_provider_profile` 失败模式

| 步骤 | 失败后会留下什么不一致状态 | 重试是否安全 | 不安全 / 部分安全时的具体后果 |
| --- | --- | --- | --- |
| 1 | 无持久化改动；只是无法读取旧 profiles，函数直接失败 | 安全 | 无 |
| 2 | 无磁盘改动，但会阻止后续保存；若当前 Codex 已在 relay 状态，`official_config_snapshot` 可能继续为空 | 安全 | 无直接副作用；只是后续仍可能走“无 snapshot 退化为自动中转”路径 |
| 3 | 最核心的不一致点：`provider-profiles.json` 已写入新/改后的 profile、active profile 和 snapshot，但还没开始应用 relay config | 安全但会放大后续副作用 | 再次重试保存同一 profile 一般不会产生重复 profile，因为用 `id` 覆盖；但如果第一次磁盘已写、第二次又改了内容，会覆盖第一次结果。更关键的是此时 UI/存储层已经认为 profile 已保存甚至已激活，而 `~/.codex/config.toml` 还没同步 |
| 4 | `provider-profiles.json` 已保存，active profile 可能已切换，但 `apply_active_profile` 失败，造成“档案状态已切换，中转未应用” | 部分安全 | 再次重试通常会再次尝试把当前 active profile 应用到 Codex 配置，最终可能收敛；但在两次重试之间，快照页/列表看到的是新 active profile，真实生效的可能仍是旧 config |
| 5 | 保存已成功且 active profile 已记录，但 `apply_active_profile` 在普通/退化中转分支里可能只改了一半：例如 `auth.json` 已写而 `config.toml` 未写，或 `config.toml` 已写而调用方收到错误 | 部分安全 | 会留下“profiles 已写但中转未应用”或“profiles 已写且部分应用”状态。重试通常不会创建重复 profile，但会继续覆盖 `auth.json`、生成更多 `.bak` 并可能掩盖第一次失败现场 |
| 6 | 保存已成功且 active profile 已记录，但官方直连恢复分支可能已写回 `config.toml`，而清理 `auth.json` 的动作失败并被吞掉 | 部分安全 | 再次重试大多可收敛，但会继续产生新 `.bak`；残留 API key 会让“已切回官方直连”与磁盘细节不完全一致 |

### 汇总判断

- `apply_provider` 的主要不一致窗口在 `relay_config` 写盘过程中，尤其是 API 模式先写 `auth.json` 再写 `config.toml`。
- `save_provider_profile` 的主要不一致窗口更早：`provider-profiles.json` 先落盘，`apply_active_profile` 后执行，所以天然存在“配置档数据库已更新，但运行态没切过去”的断点。
- 当前实现没有把 `provider-profiles.json`、`~/.codex/config.toml`、`~/.codex/auth.json` 视为一个事务单元；它们只是在同一个 `spawn_blocking` 闭包里顺序执行，但失败后不会自动 restore。

## Section 3：三个事务性方案对比

### 方案 A：写之前 snapshot 一份，失败时 rollback

#### 实现要点

基于 Section 1，至少需要对下面这些副作用对象做 snapshot：

- `provider-profiles.json`
  - 适用于 `save_provider_profile`
  - 因为它在当前实现里先于 `apply_active_profile` 落盘
- `~/.codex/config.toml`
  - 适用于 `apply_provider` 和 `save_provider_profile -> apply_active_profile`
  - 当前 `relay_config` 已经会自动生成 `.bak`，但调用方没有利用它做回滚
- `~/.codex/auth.json`
  - 适用于 API 模式写 key 和官方直连恢复时清 key
  - 当前代码对它没有显式备份，且 `clear_api_key_auth_json` 失败被吞掉

可行落点：

- 在 `provider.rs` 或 `provider_store.rs` 增加一个“事务上下文”辅助结构，先把三类文件当前内容读出来。
- 运行真实保存 / 应用步骤。
- 任一步失败时：
  - restore `provider-profiles.json`
  - restore `config.toml`
  - restore `auth.json`
- 如果 rollback 本身失败，需要把“原始错误 + 回滚失败”一起返回，避免伪装成成功。

#### 改动量估算

- 行数：中等，约 120-220 行
- 新增文件数：可以做到 0，也可以新增 1 个纯辅助模块；如果严格控范围，后续实施可先不加新文件
- 是否需要新 trait/struct：建议至少新增 1 个内部 `snapshot/rollback` struct；不一定需要 trait

#### 优点

- 对现有前后端接口侵入最小，`save_provider_profile` / `apply_provider` 的调用方式基本不用变
- 最直接解决“profiles 已写但中转未应用”这一类不一致
- 能复用现有 `relay_config` 已生成的 `.bak` 思路，但把它上升到命令层统一管理

#### 缺点

- 回滚链本身也有失败可能，尤其是 `auth.json` 和 `config.toml` 需要一起恢复时
- 会引入更多“保存前快照文件内容”的样板代码，命令函数会更长
- 如果外部有并发进程同时修改 `~/.codex/config.toml` / `auth.json`，简单 snapshot 回滚可能覆盖别人刚写入的新内容

#### 边界 case

- snapshot 自己失败怎么办？
  - 我建议直接 fail-fast，不进入真正写盘阶段；否则所谓 rollback 没有可信基线
- 原文件本来不存在怎么办？
  - snapshot 需要显式记录“文件不存在”，回滚时删除新建文件而不是写空串
- `relay_config` 自己又生成 `.bak`，会不会和上层 snapshot 重复？
  - 会重复，但可以接受；上层事务快照解决一致性，底层 `.bak` 仍可保留作人工兜底

### 方案 B：拆成“准备 + 提交”两阶段

#### 前提

- `prepare` 只做 dry-run 校验和快照准备，不落盘
- `commit` 只消费准备阶段产物并真正执行写盘

#### 接口变化

可能新增：

- `prepare_apply_provider`
- `commit_apply_provider`
- `prepare_save_provider_profile`
- `commit_save_provider_profile`

或者更抽象一点：

- `prepare_provider_transaction`
- `commit_provider_transaction`

原有命令是否保留：

- 建议短期保留原命令作为兼容包装，但内部改成“prepare + commit”
- 如果维护者愿意同步前端调用，可以逐步废弃旧命令

#### 改动量估算

- 行数：较大，约 220-400 行
- 新增文件数：大概率 1-2 个，放准备结果结构、事务 token、临时缓存逻辑
- 是否需要新 trait/struct：需要，至少要有 `PreparedProviderChange` 一类结构；如果跨命令持有准备态，还要设计存储位置和过期策略

#### 优点

- 语义最清晰：先验证“能不能做”，再真正“提交”
- 前端可以在 commit 前向用户展示更明确的风险、影响文件和回滚点
- 后续如果要加确认弹窗、差异预览、一步步应用，非常容易扩展

#### 缺点

- 改动最大，超出“修一个命令的事务性”这个小任务的常规体量
- prepare 和 commit 之间存在时间窗：prepare 时读取到的登录态、文件内容，到 commit 时可能已经变了
- 需要前端 UI 改造，命令数量、状态机和错误处理都会变复杂

#### 对现有前端 UI 流的影响

- 当前“点保存 / 点应用”是一跳式操作；两阶段后，前端至少要处理：
  - prepare 失败提示
  - prepare 成功后的确认状态
  - commit 失败后的回滚 / 重试提示
- 还要决定 prepare 结果是否短时缓存、切页面是否作废、并发点击如何处理

### 方案 C：什么都不做

#### 文档层要补什么

至少要把手工恢复路径写清楚，建议更新：

- `docs/development/refactor-backlog.md`
- 面向维护者的开发文档，例如 `docs/development/` 下专门说明 provider 流程的文档
- 如果这类故障可能暴露给最终使用者，还需要更新 README 或 manager 使用说明

文档里至少要写明：

- `provider-profiles.json` 路径
- `~/.codex/config.toml` 路径
- `~/.codex/auth.json` 路径
- `config.toml.codex-pilot-backup-*.bak` 的恢复方式
- 发生“profiles 已写但 relay 未应用”时，应该手动重新应用 active profile 还是恢复旧文件

#### 优点

- 零代码
- 没有新的实现风险，也不会把简单命令流改复杂

#### 缺点

- 用户教育成本高
- 误操作风险持续存在
- 问题不会消失，只是从“系统自动保证一致性”转成“人肉排障”
- 这类状态错位最容易让维护者误判：UI 看起来切换成功，实际 `config.toml` / `auth.json` 还没一致

## 推荐方案

维护者参考下，我更推荐 **方案 A**。

理由：

- 当前问题的核心是三个文件面之间缺少统一回滚，而不是缺少复杂的前端交互。
- 方案 A 能最直接补上事务边界，且不要求同时重做 Tauri 命令协议和前端状态流。
- 方案 B 在架构上更整洁，但对这个问题来说改动过大，容易把“修事务一致性”演变成一轮接口重设计。
- 方案 C 成本最低，但它只是把系统性风险转嫁给人工恢复，不适合作为长期默认方案。

最终是否选 A、是否先做最小 rollback 还是一步到位覆盖 `auth.json` / `config.toml` / `provider-profiles.json` 三类文件，仍建议由维护者基于后续实施预算决定。
