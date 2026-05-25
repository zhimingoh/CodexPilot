# storage.rs 拆分调研报告

目标文件：`crates/codex-pilot-data/src/storage.rs`（当前 1957 行）

本报告只基于当前实现做结构盘点和拆分风险分析，不包含任何代码改动方案承诺。

## Section 1：公共 API 列表

说明：用户要求至少覆盖所有 `pub fn` 和 `pub struct`。当前文件里还存在 1 个 `pub enum`（`DeleteStatus`），它同样属于公开 API，下面一并列出，避免遗漏真实公开面。

| 名称 | 行号 | 类型 | 功能 | 主要依赖 |
| --- | --- | --- | --- | --- |
| `SessionRef` | 12-15 | struct | 表示会话引用，只包含 `id` 和可选 `title` | `serde::{Serialize, Deserialize}` |
| `SessionRef::new` | 18-23 | fn | 构造 `SessionRef` | `SessionRef` |
| `SessionRef::normalized_id` | 25-27 | fn | 去掉 `local:` 前缀，产出统一会话 id | `normalize_session_id` |
| `DeleteStatus` | 32-37 | enum | 描述删除/撤销/未找到/失败状态 | `serde::{Serialize, Deserialize}` |
| `DeleteResult` | 40-46 | struct | 删除或撤销操作的统一返回体 | `DeleteStatus`, `std::path::PathBuf`, `serde::{Serialize, Deserialize}` |
| `DeleteResult::deleted` | 49-51 | fn | 判断结果是否为 `Deleted` | `DeleteStatus` |
| `RecycleBinEntry` | 56-68 | struct | 回收站列表项，承载备份元数据与恢复状态 | `std::path::PathBuf`, `serde::{Serialize, Deserialize}` |
| `SQLiteStorageAdapter` | 71-74 | struct | 面向 SQLite 本地存储的统一适配器，封装 DB 路径和回收站目录 | `std::path::PathBuf` |
| `SQLiteStorageAdapter::new` | 100-109 | fn | 用 DB 路径构造适配器，并推导默认 `.codex-pilot-undo` 目录 | `Path`, `PathBuf` |
| `SQLiteStorageAdapter::with_backup_dir` | 111-116 | fn | 用显式回收站目录构造适配器，主要给测试或定制路径使用 | `PathBuf` |
| `SQLiteStorageAdapter::db_path` | 118-120 | fn | 暴露当前 DB 路径 | `PathBuf` |
| `SQLiteStorageAdapter::backup_dir` | 122-124 | fn | 暴露当前回收站目录 | `Path` |
| `SQLiteStorageAdapter::delete_local` | 126-143 | fn | 根据识别到的 schema 删除本地会话/线程 | `SessionRef::normalized_id`, `rusqlite::Connection`, `schema_kind`, `delete_generic_session`, `delete_codex_thread`, `failed` |
| `SQLiteStorageAdapter::inspect_delete_local` | 145-193 | fn | 删除前做只读检查，返回 schema、命中数量和样例线程 id | `SessionRef::normalized_id`, `rusqlite::Connection`, `schema_kind`, `select_rows`, `sample_thread_ids`, `serde_json::json` |
| `SQLiteStorageAdapter::undo` | 195-224 | fn | 按 undo token 恢复 DB 行、rollout 文件和 session index | `backup_path`, `BackupPayload`, `serde_json`, `rusqlite::Connection`, `restore_tables`, `restore_files`, `restore_session_index_entries`, `std::fs` |
| `SQLiteStorageAdapter::list_undo_backups` | 226-243 | fn | 扫描回收站目录并生成回收站条目列表 | `std::fs::read_dir`, `RecycleBinEntry`, `recycle_entry_from_path` |
| `SQLiteStorageAdapter::delete_undo_backup` | 245-260 | fn | 永久删除单个 undo 备份文件 | `backup_path`, `token_session_id`, `not_found`, `DeleteResult`, `std::fs::remove_file` |
| `SQLiteStorageAdapter::find_archived_thread_by_title` | 262-288 | fn | 在 Codex thread schema 中按标题查找最近归档线程 | `rusqlite::Connection`, `schema_kind`, `has_columns`, `SessionRef::new` |
| `SQLiteStorageAdapter::move_codex_thread_workspace` | 290-386 | fn | 更新线程 `cwd`，并同步 rollout 里的 `session_meta.cwd` | `SessionRef::normalized_id`, `rusqlite::Connection`, `schema_kind`, `has_columns`, `codex_thread_timestamp_columns`, `quote_identifier`, `sql_value_to_json`, `update_rollout_session_meta_cwd`, `add_timestamp_payload`, `serde_json::json` |
| `SQLiteStorageAdapter::codex_thread_sort_key` | 388-417 | fn | 读取单个线程的排序时间字段 | `SessionRef::normalized_id`, `rusqlite::Connection`, `schema_kind`, `fetch_thread_timestamp_payload`, `serde_json::json` |
| `SQLiteStorageAdapter::codex_thread_sort_keys` | 419-457 | fn | 批量读取线程排序时间字段，去重并限制数量 | `SessionRef::normalized_id`, `rusqlite::Connection`, `schema_kind`, `fetch_thread_timestamp_payload`, `serde_json::json` |

## Section 2：功能分组建议

### 组 A：标识与结果模型

- `SessionRef`
- `SessionRef::new`
- `SessionRef::normalized_id`
- `DeleteStatus`
- `DeleteResult`
- `DeleteResult::deleted`

建议原因：这一组是最纯的数据模型和轻量行为，没有数据库连接、文件 I/O，也没有 schema 分支；它们已经被 `codex-pilot-core` 和 `codex-pilot-manager` 直接当作跨 crate 的协议类型使用，天然适合独立成稳定 API 层。

### 组 B：适配器构造与回收站入口

- `RecycleBinEntry`
- `SQLiteStorageAdapter`
- `SQLiteStorageAdapter::new`
- `SQLiteStorageAdapter::with_backup_dir`
- `SQLiteStorageAdapter::db_path`
- `SQLiteStorageAdapter::backup_dir`
- `SQLiteStorageAdapter::list_undo_backups`
- `SQLiteStorageAdapter::delete_undo_backup`

建议原因：这一组围绕“适配器对象本身”和“回收站备份文件的列举/清理”展开，主要依赖 `db_path` / `backup_dir`、备份 JSON 解析和文件系统扫描，不直接负责业务删除流程。

### 组 C：删除与恢复编排

- `SQLiteStorageAdapter::delete_local`
- `SQLiteStorageAdapter::inspect_delete_local`
- `SQLiteStorageAdapter::undo`

建议原因：这三项都是删除生命周期核心入口。`inspect_delete_local` 是只读预检，`delete_local` 是删前分流与执行，`undo` 是反向恢复。它们共用 schema 识别、行级备份恢复、文件恢复和索引恢复逻辑，拆开后反而会把一次删除事务打散。

### 组 D：Codex 线程扩展操作

- `SQLiteStorageAdapter::find_archived_thread_by_title`
- `SQLiteStorageAdapter::move_codex_thread_workspace`
- `SQLiteStorageAdapter::codex_thread_sort_key`
- `SQLiteStorageAdapter::codex_thread_sort_keys`

建议原因：这一组都只对 `CodexThreads` schema 有意义，且共享 `threads` 表字段约束、时间戳提取和 thread id 归一化逻辑，和 generic session 删除链路的耦合明显更弱。

## Section 3：组间依赖

### A 组 -> C 组

- `SQLiteStorageAdapter::delete_local` 调用 `SessionRef::normalized_id`。
- `SQLiteStorageAdapter::inspect_delete_local` 调用 `SessionRef::normalized_id`。
- `SQLiteStorageAdapter::undo` 返回 `DeleteResult` / `DeleteStatus`。

影响：A 组基本是 C 组的入参/出参层，拆分时应保持它最稳定，避免把模型类型跟重 I/O 实现绑在同一个文件。

### A 组 -> D 组

- `SQLiteStorageAdapter::find_archived_thread_by_title` 构造 `SessionRef::new(...)` 作为返回值。
- `SQLiteStorageAdapter::move_codex_thread_workspace`、`codex_thread_sort_key`、`codex_thread_sort_keys` 都依赖 `SessionRef::normalized_id`。

影响：D 组也依赖 A 组作为 thread 级 API 的统一参数/返回类型，因此 A 组更像底层公共模型模块。

### B 组 -> C 组

- `SQLiteStorageAdapter::delete_local`、`inspect_delete_local`、`undo` 都是 `SQLiteStorageAdapter` 的实例方法，依赖 B 组里的适配器状态。
- `SQLiteStorageAdapter::undo` 通过 `backup_path` 读回 B 组所管理的 undo 文件目录。

影响：如果未来把方法按模块分拆，`SQLiteStorageAdapter` 本体大概率要留在更薄的一层，由各子模块继续给它补 `impl`，否则会被迫在组间来回传 `db_path` / `backup_dir`。

### B 组 -> D 组

- D 组全部 API 也是 `SQLiteStorageAdapter` 的实例方法，直接使用 B 组管理的 `db_path`。

影响：D 组可以单独成 `thread_ops` 风格模块，但不适合把适配器结构本体一起搬走。

### C 组内部依赖与共享底座

- `delete_local` 按 `schema_kind` 分流到私有 helper `delete_generic_session` / `delete_codex_thread`。
- `undo` 依赖 `write_backup` 的产物格式，以及 `restore_tables` / `restore_files` / `restore_session_index_entries` 的恢复顺序契约。
- `inspect_delete_local` 与 `delete_local` 共用 `schema_kind`、`select_rows` 这套 schema/查询 helper。

影响：C 组对私有 helper 的共享最多，是拆分时最容易把“只在本文件可见”的函数误拆成跨模块公共函数的一组。

### D 组内部依赖与共享底座

- `move_codex_thread_workspace` 与 `codex_thread_sort_key` / `codex_thread_sort_keys` 共用时间戳 helper：`codex_thread_timestamp_columns`、`fetch_thread_timestamp_payload`、`add_timestamp_payload`。
- `move_codex_thread_workspace` 还额外依赖 `update_rollout_session_meta_cwd` 这类文件回写 helper。

影响：D 组内部 cohesive 比较高，适合整体迁移；如果只拆一个函数，会把 thread 专属 helper 留成悬空公共杂项。

### 循环依赖检查

- 公开 API 组之间没有发现 A ↔ B、B ↔ C、C ↔ D 这种互相调用的循环依赖。
- 当前更真实的耦合模式是“B 组的适配器承载状态，A 组提供模型，C/D 组作为不同业务域挂在同一个 `impl SQLiteStorageAdapter` 上”。
- 因此，拆分难点不在公开 API 互调，而在大量私有 helper 被 C 组和 D 组分别、且有时交叉地共享。

## Section 4：拆分风险清单

### 1. 私有 helper 是否被多组共用？拆分后是否需要变成 `pub`？哪些？

有，而且不少，但我的判断是“优先变成 `pub(crate)` 或保持私有到子模块”，不该直接升成对外 `pub`。

多组共用最明显的私有 helper：

- schema/查询层：`schema_kind`、`has_table`、`has_columns`、`table_columns`、`select_rows`
- SQL/值转换层：`sql_value_to_json`、`json_to_sql_value`、`quote_identifier`、`OwnedSqlValue`
- 删除结果组装：`deleted`、`not_found`、`failed`、`failed_with_backup`
- 线程时间戳层：`codex_thread_timestamp_columns`、`fetch_thread_timestamp_payload`、`add_timestamp_payload`
- 备份/恢复层：`restore_tables`、`restore_table_order`、`validate_restore_tables`、`insert_row`、`update_existing_agent_job_item`
- 文件/索引层：`session_index_path`、`session_index_line_id`、`remove_session_index_entries`、`restore_session_index_entries`、`update_rollout_session_meta_cwd`

风险点：

- 如果按“每组一个文件”硬拆，这些 helper 很可能被复制，后续行为漂移。
- 如果为了复用直接升成对外 `pub`，会把当前只是实现细节的 schema/备份格式固定成外部契约，代价过高。

更稳妥的方向：

- 先把它们收敛成 `storage` 模块内部子模块共享的 `pub(crate)` / `pub(super)` helper。
- 只保留现有 public API 面，不额外扩散新公开符号。

### 2. 共享的 `const` / `static` / type alias 有哪些？拆完放哪？

严格说，文件级共享 `const` / `static` / type alias` 几乎没有，但有几个“等价于共享基础类型/常量角色”的内部元素：

- `SchemaKind`：schema 分流的核心内部类型，C 组和 D 组都会用到
- `BackupPayload`：undo 备份 JSON 的内部载体，B 组和 C 组会共用
- `OwnedSqlValue`：恢复写库时的桥接类型，属于 restore/SQL helper 底座
- `encode_hex` 内部的局部常量 `HEX`：只服务 blob 编码，不需要外提
- `restore_table_order` 里的 `preferred` 数组：是恢复顺序契约，本质上是备份恢复模块的局部常量

风险点：

- 虽然没有显式的全局 `const`，但 `SchemaKind` 和 `BackupPayload` 实际上是跨多块逻辑共享的“内部协议”。
- 如果把它们随意放到某一个业务文件里，其他组会反向依赖过去，制造新的中心文件。

建议落点：

- `SchemaKind` 放到 `storage::schema` 或 `storage::common` 一类内部模块。
- `BackupPayload` 放到 `storage::backup` / `storage::undo` 一类内部模块。
- `OwnedSqlValue` 和值转换函数放到更底层的 `storage::sql` / `storage::restore_support`。

### 3. 测试模块（`#[cfg(test)] mod tests`）跟拆的策略：跟谁走？还是单独建一个 `storage_tests.rs`？

当前测试明显不是单一功能测试，而是跨组集成测试：

- generic session 删除/恢复
- 回收站列举/永久删除
- FK 顺序恢复
- codex thread 删除/恢复 rollout 文件
- session index 删除/恢复
- archived thread 查找、workspace 迁移、sort key 读取
- recycle entry 的 `project_cwd` / `last_active_at` 提取

风险点：

- 如果测试跟着单个子模块走，会出现大量 `use super::super::*` 或重复造 fixture。
- 尤其 `deletes_codex_thread_fixture`、`deletes_and_restores_codex_session_index_entry` 这种本来就在同时覆盖 DB + 文件 + 索引行为，不适合塞进某个纯 helper 测试文件。

我的判断：

- 不建议第一步就拆成单独 `storage_tests.rs`，因为那会额外引入测试可见性和模块路径整理工作，超出 T09 的“先调研”边界。
- 更合适的是后续真拆实现时，把“跨模块集成测试”保留在 `storage/mod.rs` 或顶层 `storage.rs` 的测试区；只有纯 helper 单测再下沉到各子模块同文件测试里。

### 4. 是否有大的 helper 函数（>100 行）天然适合保留在原文件，还是必须拆？

有一个最明显的大函数：

- `delete_codex_thread`：490-600，约 111 行

它的问题不是单纯“太长”，而是同时负责编排：

- 线程主表和关联表备份
- rollout 文件备份/删除
- session index 备份/删除
- SQLite 事务删除
- 删除后错误聚合和回收站结果返回

风险点：

- 如果硬保留在原文件，而周边 helper 已经搬到别处，最终 `storage.rs` 会变成一个超长 orchestrator 壳子，阅读成本依旧高。
- 但如果不先抽出文件/索引/表恢复 helper，就直接把它切碎，也容易把“删除一次 thread 的完整顺序”打散，维护者反而更难看清主流程。

我的判断：

- 这个函数后续应该拆，但应拆成“主编排 + 专项 helper”两层，而不是原样长期留在根文件。
- 第一阶段最合理的落法是让它留在“删除与恢复编排组”的核心模块里，周边把文件备份、session index、关联表备份 helper 抽成内部子函数/子模块。

### 5. 拆完后 `lib.rs` 的 `pub use` 导出策略：扁平 re-export 还是按模块路径？

现状是：

- `crates/codex-pilot-data/src/lib.rs` 只暴露 `pub mod storage;`
- 外部调用已经写成 `codex_pilot_data::storage::SQLiteStorageAdapter`、`codex_pilot_data::storage::SessionRef`、`codex_pilot_data::storage::RecycleBinEntry`、`codex_pilot_data::storage::DeleteStatus`

风险点：

- 如果拆完改成 `codex_pilot_data::storage::thread_ops::...` 之类的深路径，`codex-pilot-core` 和 `codex-pilot-manager` 现有调用点都会被迫改路径，收益很小。
- 如果在 `lib.rs` 顶层再扁平 re-export 成 `codex_pilot_data::SessionRef`，又会扩大 crate 的公开命名空间，和当前使用习惯不一致。

我的判断：

- 优先保持 `codex_pilot_data::storage::*` 这层对外路径不变。
- 即使内部拆成 `storage/models.rs`、`storage/undo.rs`、`storage/thread_ops.rs`、`storage/schema.rs` 等，也建议由 `storage/mod.rs` 做扁平 re-export。
- 不建议把 re-export 提到 `lib.rs` 顶层；当前外部调用已经稳定依赖 `storage` 命名空间，这个边界本身就是有价值的。

## 结论摘要

- 真正适合先拆的是“内部 helper 分层”，不是先改公开 API。
- 公开面目前没有明显循环依赖，最该守住的是 `codex_pilot_data::storage::*` 这层稳定导出。
- 后续如果维护者决定开拆，优先顺序建议是：A 组模型稳定化 -> D 组 thread 专属逻辑独立 -> C 组删除/恢复编排收拢 -> 最后再处理 shared helper 落点。
