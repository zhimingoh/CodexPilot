# T16 · session_zip merge 会话不显示 + 导出顺序调研

目标文件：

- `crates/codex-pilot-data/src/session_zip.rs`
- `crates/codex-pilot-data/src/storage.rs`
- `crates/codex-pilot-core/src/routes_sessions.rs`
- `crates/codex-pilot-core/src/bridge.rs`

本报告只基于当前 manager 侧真实实现，回答 merge 导入后会话在 Codex 历史列表里偶发不可见、以及 ZIP 导出顺序与 UI 不一致这两个问题。文中不假设 Codex 产品内部如何渲染历史，只描述 manager 端可观察的输入、查询和返回结果。

## Section 1：副作用与状态边界

先给出 `session_zip` 四个流程的状态读写边界。入口都在 `SessionZipService`：导出 `export_current_state[_to_path]`（`session_zip.rs:85-146`）、检查 `inspect_zip`（`session_zip.rs:148-158`）、导入 `import_zip`（`session_zip.rs:160-240`）。

| 流程 | 读 | 写 | 不动 |
| --- | --- | --- | --- |
| export | 读取 `~/.codex/sessions/`、`~/.codex/archived_sessions/`、`~/.codex/state_5.sqlite` 是否存在并拷入 ZIP（`session_zip.rs:103-120`）；如果导出到 manager 管理目录，还会读取 `~/.codex/backups_state/session-zip/` 现有 ZIP 以便裁剪（`session_zip.rs:139-141`, `session_zip.rs:507-521`） | 创建 ZIP 文件本身；写入 `manifest.json`（`session_zip.rs:98-138`）；在 manager 管理目录下可能删除较旧 ZIP，只保留 5 份（`session_zip.rs:507-521`） | 不修改 `~/.codex/sessions/`、`~/.codex/archived_sessions/`、`~/.codex/state_5.sqlite` 原内容。潜在不一致是：ZIP 只是某一时刻快照，导出顺序来自 `fs::read_dir`，与 UI 排序无绑定（`session_zip.rs:344-360`） |
| inspect | 读取 ZIP、解析 `manifest.json`、扫描 ZIP entries（`session_zip.rs:148-158`, `session_zip.rs:274-317`） | 不写任何持久状态 | 不修改 `~/.codex/sessions/`、`~/.codex/archived_sessions/`、`~/.codex/state_5.sqlite`、`~/.codex/backups_state/session-zip/`。潜在不一致是：inspect 只确认 ZIP 里“包含了什么”，不验证本地 sqlite 与 rollout 数据是否能对齐 |
| import(merge) | 先 inspect ZIP（`session_zip.rs:165`），再把 ZIP 解到临时目录（`session_zip.rs:172-175`, `session_zip.rs:380-415`）；读取 ZIP 中 `sessions/`、`archived_sessions/` 内容（`session_zip.rs:178-186`） | 只向本地 `~/.codex/sessions/`、`~/.codex/archived_sessions/` 复制文件（`session_zip.rs:178-186`, `session_zip.rs:438-477`） | 明确不动 `~/.codex/state_5.sqlite`，返回值把 `restored_state_sqlite` 固定成 `false`（`session_zip.rs:176-188`），消息里也写明 “state_5.sqlite 保持不变”（`session_zip.rs:210-214`）；也不创建 `~/.codex/backups_state/session-zip/` 安全备份。潜在不一致是：rollout jsonl 已导入，但 `threads` 表和 `session_index.jsonl` 仍是旧状态，manager 后续按 sqlite 查 sort_key 时可能拿不到新导入线程 |
| import(overwrite) | 先 inspect ZIP（`session_zip.rs:165`）；再先导出当前本地状态到 `~/.codex/backups_state/session-zip/` 作为 safety backup（`session_zip.rs:166-170`）；之后解压 ZIP（`session_zip.rs:172-175`）并读取 ZIP 内全部三类对象 | 覆盖 `~/.codex/sessions/`、`~/.codex/archived_sessions/`、`~/.codex/state_5.sqlite`（`session_zip.rs:189-207`, `session_zip.rs:446-489`）；同时写入一份自动安全备份 ZIP（`session_zip.rs:166-170`, `session_zip.rs:85-89`） | 不保留本地原先的 sessions / archived_sessions / sqlite 内容。潜在不一致是：overwrite 会把 sqlite 与 rollout 一起恢复，但仍不处理 ZIP 内文件顺序与 UI 排序的一致性；另外回滚粒度只有整包 safety backup，没有分步骤原子性 |

补充两条直接证据：

- `session_zip.rs:212-214` 的 merge 成功消息已经把 “state_5.sqlite 保持不变” 写死。
- 测试 `merge_does_not_replace_state_sqlite` 也明确断言 merge 后本地 sqlite 仍是 `local`，`restored_state_sqlite == false`（`session_zip.rs:613-639`）。

## Section 2：codex 历史显示链路

### 2.1 bridge 到 manager 端点

bridge 注入时会在页面里挂一个 `window.__codexPilotBridge(path, payload)`，它把 `{ id, path, payload }` 发给绑定名对应的原生 callback（`bridge_scripts.rs:3-29`）。`bridge.rs` 负责通过 CDP 注入这个 binding 和脚本（`bridge.rs:28-57`）。

manager 端收到请求后，`handle_bridge_request` 按 path 分发。和排序相关的两个入口分别是：

- `"/session/thread-sort-key"` -> `thread_sort_key(ctx, payload).await`（`routes.rs:24-25`, `routes.rs:50-52`）
- `"/session/thread-sort-keys"` -> `thread_sort_keys(ctx, payload).await`（`routes.rs:24-25`, `routes.rs:50-52`）

`BridgeContext::new` 把 `db_path` 固定为 `crate::app_paths::codex_state_db_path()`，也就是 manager 后续总是查当前本机 `state_5.sqlite`（`routes.rs:14-21`）。

### 2.2 manager 端点到 storage 查询

单个查询链路：

1. `routes_sessions::thread_sort_key` 从 payload 解析 session id（`routes_sessions.rs:149-152`）。
2. 创建 `SQLiteStorageAdapter::new(ctx.db_path)`（`routes_sessions.rs:153`）。
3. 用 `spawn_blocking` 调 `adapter.codex_thread_sort_key(&session)`（`routes_sessions.rs:154-159`）。

批量查询链路：

1. `routes_sessions::thread_sort_keys` 从 payload 里的 `sessions` 数组解析出 `Vec<SessionRef>`（`routes_sessions.rs:162-172`）。
2. 创建 `SQLiteStorageAdapter::new(ctx.db_path)`（`routes_sessions.rs:173`）。
3. 用 `spawn_blocking` 调 `adapter.codex_thread_sort_keys(&sessions)`（`routes_sessions.rs:174-179`）。

### 2.3 storage 如何查 sqlite

`codex_thread_sort_key` 会先做三层判断：

1. sqlite 文件不存在则直接返回 failed（`storage.rs:388-396`）。
2. schema 不是 `CodexThreads` 也直接 failed（`storage.rs:397-404`）。
3. 真正查询时调用 `fetch_thread_timestamp_payload(&db, &thread_id)`（`storage.rs:405-416`）。

`fetch_thread_timestamp_payload` 的行为是：

- 先从 `threads` 表现有列里筛出 `updated_at`、`updated_at_ms`、`created_at_ms` 这三个候选时间列里实际存在的部分（`storage.rs:1173-1182`）。
- 再执行 `SELECT ... FROM threads WHERE id = ?1`（`storage.rs:1184-1205`）。
- 查到行则只回填时间字段；查不到 `QueryReturnedNoRows` 时返回 `Ok(None)`（`storage.rs:1206-1214`）。

`codex_thread_sort_keys` 在批量模式下也是同一条查询逻辑，只是多了一层去重和截断：

- 先把 session ids 归一化后 fold 成 `thread_ids`，条件是 `!acc.contains(&id) && acc.len() < 200`（`storage.rs:427-436`）。
- 然后逐个 thread id 调 `fetch_thread_timestamp_payload`（`storage.rs:449-454`）。
- 只有 `if let Some(mut payload)` 才 push 到 `sort_keys` 返回数组里（`storage.rs:449-454`）。

### 2.4 哪一步丢 session

manager 端可观察到的“丢 session”有两种，且都发生在 manager 自己返回 `sort_keys` 这一步：

1. sqlite 缺行时丢。
   `fetch_thread_timestamp_payload` 对 `SELECT ... FROM threads WHERE id = ?1` 查不到行会返回 `None`（`storage.rs:1199-1214`），而 `codex_thread_sort_keys` 只在 `if let Some` 分支里 push 结果（`storage.rs:449-454`）。因此 merge 导入后如果 rollout jsonl 已复制进 `~/.codex/sessions/`，但本机 `threads` 表没有对应 id，该 session 在 manager 返回体里就根本没有对应 `sort_keys` 条目。

2. 超过 200 个时丢。
   `codex_thread_sort_keys` 在构造 `thread_ids` 时有 `acc.len() < 200` 的硬编码上限（`storage.rs:431-433`）。第 201 个及之后的唯一 id 不会进入查询阶段，因此同样不会出现在 manager 返回的 `sort_keys` 里。

换句话说，manager 侧并不返回“这个 session 缺 sort_key 的显式占位对象”，而是直接返回一个缺项数组：

- 单个接口 `thread_sort_key`：查不到时返回 `{ status: "failed", session_id, message: "thread not found in local storage" }`（`storage.rs:411-415`）。
- 批量接口 `thread_sort_keys`：查不到时静默跳过，只返回 `{ status: "ok", sort_keys: [...] }`，缺失 id 不在数组内（`storage.rs:449-456`）。

这已经足够解释“merge 导入后 Codex 历史列表里有时看不到这些会话”：不需要假设 Codex 内部排序逻辑，只要 manager 返回的 `sort_keys` 缺了这些 id，前端可用的排序元数据就不完整。

## Section 3：merge 同步 sqlite 三个方案对比

### 方案 A：merge 模式也覆盖 `state_5.sqlite`

#### 实现要点

- 直接把 merge 分支从“只 merge 两个目录”改成也执行 `overwrite_file(extracted.state_sqlite, ~/.codex/state_5.sqlite)`，即行为向 overwrite 靠拢（对照当前 merge / overwrite 分支：`session_zip.rs:176-207`）。
- 因为本地 sqlite 会被整体替换，所以 `threads` 表、以及 `schema_kind` 所识别到的 schema 版本会完全以 ZIP 里那份为准（`storage.rs:700-710`）。

#### 改动量估算

- 改动量最小。
- 主要集中在 `session_zip.rs` 的 merge 分支和返回 message。

#### 优点

- 立刻解决 “rollout 导入了但 sqlite 没同步” 这一主因。
- 不需要设计 thread 级合并算法，也不需要改 bridge 端点返回格式。

#### 缺点

- 这已经不再是“merge”语义，而是“目录 merge + sqlite overwrite”。
- 本地现有 `threads`、`thread_goals`、`agent_job_items` 等数据会被整库替换，冲突面最大；这些表正是 `delete_codex_thread` / `undo` 当前在本地备份恢复时会显式处理的对象（`storage.rs:501-572`, `storage.rs:805-915`）。
- 当前 merge 不创建 safety backup，直接覆盖 sqlite 风险过高；如果要这样做，实际上需要把 overwrite 的安全备份逻辑一起搬过去（`session_zip.rs:166-170`）。

#### 边界 case

- `schema_kind` 只接受 `GenericSessions` 或带 `id/title/rollout_path` 的 `CodexThreads`（`storage.rs:700-710`）。ZIP 里的 sqlite 如果 schema 偏旧或偏新，整库覆盖后可能让 manager 后续所有 thread 相关桥接都退成 `unsupported local storage schema`。
- 同 id 本地和 ZIP 都存在时，没有 thread 级冲突策略，ZIP 整库直接赢。
- 失败原子性较弱：目录 merge 已经发生后再覆盖 sqlite，如果中间失败，会得到“文件新、sqlite 旧”或“sqlite 新、部分文件旧”的混合态。

### 方案 B：从 ZIP 提取 sqlite 行做 `INSERT OR REPLACE`

#### 实现要点

- import 时额外打开 ZIP 内 `state_5.sqlite`，按 `threads` 主表及其关联表做行级导入，而不是整文件覆盖。
- 可参考 `delete_codex_thread` 当前已经显式识别的关联表范围：`thread_dynamic_tools`、`thread_goals`、`thread_spawn_edges`、`stage1_outputs`、`agent_job_items`（`storage.rs:504-538`, `storage.rs:553-570`）。
- 为兼容 schema 多版本，需要像 `schema_kind` / `has_table` / `has_columns` 那样先检查目标库和源库实际有哪些表列（`storage.rs:700-742`）。

#### 改动量估算

- 改动量最大。
- 不仅要改 `session_zip.rs`，还要补一套 sqlite-to-sqlite merge helper，并处理事务、列差异和关联表冲突。

#### 优点

- 语义上最接近真正的 “merge”：只把 ZIP 里有的线程补进本地 sqlite。
- 可以更细粒度地定义冲突策略，例如“同 id 已存在则跳过 / replace / 只补时间字段”。

#### 缺点

- 设计复杂度最高，尤其是 `agent_job_items` 已有恢复冲突逻辑，说明 thread 相关数据并不是简单 `INSERT OR REPLACE` 就能无脑合并（`storage.rs:918-947`）。
- 如果 ZIP 源库和本地库 `threads` 列不同，单纯 `INSERT OR REPLACE` 很容易踩到缺列或约束问题。
- 失败原子性要自己补：至少要保证“本地 sqlite 行合并失败时，rollout 文件也不能只 merge 一半”。

#### 边界 case

- `SchemaKind::GenericSessions` vs `SchemaKind::CodexThreads` 混用时，merge 逻辑必须先拒绝或分流，不能默认 threads 一定存在（`storage.rs:700-710`）。
- 同 id 冲突策略必须明说：本地已有线程且 ZIP 里也有时，究竟谁赢；特别是时间戳列 `updated_at` / `updated_at_ms` / `created_at_ms` 可能互相不完整（`storage.rs:1173-1224`）。
- 是否需要先 backup：我认为需要。因为一旦写错目标 sqlite，回滚成本远高于 rollout 文件复制。

### 方案 C：保持 import 不动，改造 sort_key 查询，在 sqlite 缺行时从 rollout jsonl 兜底

#### 实现要点

- 不改 `import_zip` merge 行为，保留“只复制 sessions / archived_sessions，不动 sqlite”（`session_zip.rs:176-188`）。
- 在 `codex_thread_sort_key` / `codex_thread_sort_keys` 或 `fetch_thread_timestamp_payload` 周边增加兜底：当 sqlite `threads` 缺行时，改从 rollout jsonl 读取可用时间戳。
- 兜底所需输入已经在 manager 请求里有 thread id；manager 也知道 `~/.codex/sessions/` 是 merge 的落点（`session_zip.rs:179-186`）。但需要新增“如何由 id 找到 rollout 文件”的规则。

#### 改动量估算

- 中等。
- 不需要动导入主流程，但需要扩展 storage 查询层，并且可能要增加基于文件系统的扫描与解析。

#### 优点

- 直接修复当前用户可见问题点：即使 sqlite 缺行，也尽量给 bridge 返回 sort_key。
- 不会改变 merge 的磁盘副作用边界，风险小于 A/B。
- 对 schema 多版本更友好，因为它绕开了 sqlite 行合并。

#### 缺点

- 这属于读时兜底，不会真正补齐 sqlite。manager 其余依赖 `threads` 表的能力仍可能继续看不到这些会话，例如 `move_codex_thread_workspace` 查询 thread 行失败时就会报 `thread not found in local storage`（`storage.rs:339-355`）。
- 需要定义“从 rollout 文件推时间戳”的可靠来源；当前 storage 里只有更新 rollout `session_meta.cwd` 的 helper，并没有现成的时间戳解析逻辑（`storage.rs:1132-1171`）。
- 如果 session 文件很多，批量 sort_key 接口可能从“查 sqlite”退化成“查大量文件”，性能和 bridge 超时都要重评估。

#### 边界 case

- schema 兼容性相对简单，因为只在 sqlite 缺行时兜底；但 `GenericSessions` 场景下是否也要兜底，需要单独决策。
- 同 id 冲突策略：若 sqlite 有行且 rollout 也有文件，应以 sqlite 还是 rollout 为准。当前最稳妥是“有 sqlite 就仍以 sqlite 为准”。
- 是否需要 backup：不需要，因为这是读路径改造。
- 失败原子性：读时兜底天然没有写事务，但需要把“文件损坏/解析失败”明确体现在返回上，不能静默伪造时间。

## Section 4：200 上限分析

目标代码在 `codex_thread_sort_keys`：

```rust
fold(Vec::<String>::new(), |mut acc, id| {
    if !acc.contains(&id) && acc.len() < 200 {
        acc.push(id);
    }
    acc
});
```

对应行号是 `storage.rs:427-436`，其中硬编码上限是 `storage.rs:432`。

### 4.1 可能来源逐项判断

#### SQLite 参数数量限制？

反证更强。

- 当前实现根本不是一条 `WHERE id IN (?, ?, ...)` 的批量 SQL，而是先构造 `thread_ids`，再逐个 thread id 调 `fetch_thread_timestamp_payload`（`storage.rs:449-454`）。
- `fetch_thread_timestamp_payload` 本身每次只执行一条 `WHERE id = ?1` 的单参数查询（`storage.rs:1191-1214`）。

因此 `200` 不是为了躲 SQLite variable number limit。

#### 防止超大请求拖慢 bridge？

有一定支持，但证据只到“可能是防御性限流”，没有更明确注释。

- `thread_sort_keys` 是 bridge 入口，会通过 `spawn_blocking` 在 manager 侧同步查 sqlite（`routes_sessions.rs:162-179`）。
- 当前实现对每个 id 单独查一次 sqlite（`storage.rs:449-454`），请求量线性放大，所以作者很可能想避免一次带入过多 id。

但反证也存在：

- 代码没有注释、没有命名常量、没有错误提示，只是静默截断（`storage.rs:431-433`）。
- 既然是性能保护，通常更自然的做法会是显式返回 “truncated: true” 或分批查询；当前返回体没有任何截断信息（`storage.rs:456`）。

#### 历史代码遗留 magic number？

支持较强。

- 代码里没有常量名、没有文档、没有测试专门解释 200 的业务含义。
- 该值既不对应 sqlite 参数上限，也不对应当前任何 UI page size 常量。更像一次经验性防御值，后续没有继续抽象。

#### 防御性截断，避免攻击面？

有一定支持。

- bridge 暴露的是可被页面脚本调用的接口，理论上应防止异常大 payload 把 manager 拖慢。`thread_sort_keys` 目前唯一的输入体量控制就是这个 200 上限（`routes_sessions.rs:162-179`, `storage.rs:427-436`）。

但同样缺少佐证：

- 没有记录日志，没有返回 `too_many_sessions` 之类的错误或告警。
- 对攻击面防护来说，`acc.contains` 还是 `O(n)` 去重，限制写法并不完整。

### 4.2 建议

我不建议完全去除，也不建议维持现状不动。

更合适的路线是：

1. 保留“bridge 请求需要有上限”的防御思路。
2. 但把 `200` 从静默 magic number 改成显式常量或分批查询策略。
3. 优先方案是分批查询或至少显式截断返回，而不是现在这样直接丢第 201 个及之后的 id。

原因：

- 当前 200 的真实副作用不是“慢一点”，而是“manager 返回缺条目”，这正好和本次现象叠加。
- 即使 merge 问题另修，现有批量接口仍会在大列表下静默漏掉后面的会话。

如果只在 “保留 / 提高到 N / 改成分批查询 / 完全去除” 四个选项里选，我倾向于：

- 首选：改成分批查询。
- 次选：暂时保留上限，但把结果体补成显式截断。
- 不建议：完全去除。

## Section 5：导出顺序

### 5.1 为什么 ZIP 内文件顺序不稳定

导出目录时，`add_directory_recursive` 直接遍历 `fs::read_dir(current)?`，随后遇到文件就立刻 `writer.start_file(zip_name, options)` 写入 ZIP（`session_zip.rs:336-360`）。代码里没有任何排序步骤。

Rust 标准库对 `read_dir` 的说明已经明确写了两点：

- “The order in which `read_dir` returns entries can change between calls.”
- “The order in which this iterator returns entries is platform and filesystem dependent.”

来源：[Rust std::fs::read_dir](https://doc.rust-lang.org/std/fs/fn.read_dir.html)。

因此对 macOS APFS、Linux ext4、Windows NTFS，manager 当前都不能假定 `read_dir` 有稳定顺序。差别不在“哪个平台一定有序”，而在“标准库接口统一不保证有序，且依赖底层平台和文件系统实现”。

### 5.2 如果要排序，应该加在哪一步

最直接的位置就是 `add_directory_recursive` 和 `copy_directory_contents` 里，在拿到 `read_dir` 结果后先 collect 成 `Vec<PathBuf>`，按目标规则排序，再递归/写 ZIP：

- 导出 ZIP 顺序：`session_zip.rs:344-360`
- merge / overwrite 复制目录时的顺序：`session_zip.rs:458-476`

如果只想解决“ZIP 内条目顺序稳定”，只改导出路径即可；如果还想让导入时的覆盖顺序也可预测，则复制目录的 helper 也应同步排序。

### 5.3 应按什么排序

从当前 manager 代码看，没有现成证据支持“创建时间”或“文件系统 mtime”能代表 UI 顺序。相反，manager 当前 UI 相关排序依据更偏业务时间：

- recycle bin 列表按 `deleted_at DESC`（`storage.rs:226-242`）。
- 调试采样 `sample_thread_ids` 按 `updated_at_ms DESC, updated_at DESC, created_at_ms DESC`（`storage.rs:687-697`）。
- thread sort_key 返回的也是 `updated_at` / `updated_at_ms` / `created_at_ms`（`storage.rs:1173-1224`）。

但 ZIP 导出函数当前只看目录树，不读 sqlite 时间戳，也不解析 rollout 文件。因此如果目标是“与 UI 一致”，单靠 `add_directory_recursive` 层面只能稳定成某个机械顺序，例如：

- 文件名排序
- 相对路径排序
- 文件 metadata 的 mtime 排序

其中最稳妥的是相对路径 / 文件名排序，因为：

- 不依赖平台差异更大的文件时间精度。
- 只需要在现有目录遍历处加 sort。
- 对 ZIP 可重复性最好。

### 5.4 这种排序对 Codex 行为和人工查看的影响

- 对 Codex 行为：本报告没有发现 manager 侧任何代码在导入时依赖 ZIP 内部 entry 顺序。导入会把 ZIP 全量解压到临时目录，再按目录复制或覆盖（`session_zip.rs:172-207`, `session_zip.rs:380-489`）。所以条目顺序更像可观察性问题，而不是功能正确性问题。
- 对人工 unzip 查看：有影响。当前顺序不稳定，会让同一批文件每次导出的 ZIP 目录视图不同，不利于肉眼 diff、排查和审计。

### 5.5 能不能与 manager UI 排序一致

不能直接一致，至少当前实现做不到。

原因有两层：

1. manager UI 相关排序主要依赖 sqlite 里的业务时间列，而不是目录树顺序（`storage.rs:687-697`, `storage.rs:1173-1224`）。
2. `session_zip` 导出目录时并没有把每个 rollout 文件先映射回 `threads` 表记录，所以它不知道对应会话的 `updated_at_ms`。

因此：

- 如果只在 `add_directory_recursive` 层加 sort，最多做到“ZIP 顺序稳定”，不能保证“与 UI 顺序一致”。
- 如果真要追求“与 UI 一致”，导出路径需要先构建“session file -> thread row 时间戳”的映射，这已经不是简单目录排序改动。

## Section 6：推荐方案

### 优先级排序

1. 优先修 merge 后 sort_key 缺失。
2. 第二优先处理 `thread_sort_keys` 的 200 静默截断。
3. 最后再考虑 ZIP 内文件顺序稳定化。

### 推荐组合

#### 对“merge 导入后会话不显示”

首推 **方案 C 作为止血**，再评估 **方案 B 作为长期修法**。

- 方案 C 的 ROI 最高：不改变 import 副作用边界，就能直接改善 manager 返回 `sort_keys` 缺项的问题。
- 但它只是读时兜底，不能替代 sqlite 真正同步；如果后续希望 `move_thread_workspace`、删除、归档查找等依赖 `threads` 表的能力也对 merge 导入线程生效，最终仍要走方案 B 这一类“补齐 sqlite”的路线。

不推荐直接上方案 A：

- 它最省代码，但会把 merge 语义变得不再可预期，而且没有配套 safety backup 的话风险过高。

#### 对“200 上限”

建议 **不要保持现状**。

- 近期路线：先把静默截断改成显式可见。
- 中期路线：改成分批查询，避免把请求体量控制和结果缺项绑死在一起。

#### 对“导出顺序”

建议 **修稳定性，但不把它当成本次最高优先级**。

- 先做按相对路径 / 文件名排序，解决“同一份数据多次导出顺序不稳定”的问题。
- 暂不承诺“与 UI 一致”，因为当前导出层没有足够上下文做到这一点。

### 最终修法路线

我建议的总体路线是：

1. 先在 sort_key 查询链路补兜底或显式缺项表达，解决 merge 导入后 manager 返回不完整的问题。
2. 随后把 `thread_sort_keys` 的 200 上限改成显式、可维护的批量策略。
3. 最后给 ZIP 导出目录遍历加稳定排序，把“无序”问题收敛成可重复输出。
4. 如果后续确认 merge 导入的线程需要在 manager 全功能层面与本地原生线程等价，再单开任务评估方案 B 的 sqlite 行级合并。

这个顺序的原因是：

- 第 1、2 步直接影响“为什么看不到会话”这个主问题，用户感知最强。
- 第 3 步主要影响可观察性和人工排查体验，不影响当前导入正确性。
- 第 4 步收益大，但设计和验证成本也最高，适合在止血后单独推进。
