# 数据库工作区设计(2026-08-18)

**状态:** 已与用户确认
**范围:** 数据库模块完善--只读查询 + 库/表浏览、查询结果送入数据分析、连接健康持久化 + 表单补全、设置页标签导航

## 背景与目标

Bloomery 已有 SQL Server 连接管理(`database_connections` 表、keyring 密码、tiberius 连接层、设置页面板),但只能测试连通和列表名。本设计把它升级为完整的数据工作区:用户在一级导航的「数据库」分区里浏览库表、执行受控只读查询、把结果送入数据分析的训练/优化流程;连接的增删改留在设置页,设置页同时引入内部标签导航解决拥挤问题。

**已确认的关键决策:**

| 决策 | 结论 | 理由 |
| --- | --- | --- |
| 查询工作台归属 | 一级导航「数据库」(知识库与数据分析之间) | 查询/浏览是工作面,与知识库对称;配置属于设置 |
| 连接管理归属 | 留在设置页「数据库」标签 | 一处配置,两处使用(与 Provider 先例一致) |
| 查询执行模型 | Rust scheduler 后台任务(kind `database_query`) | 可取消、有进度、进任务列表、耗时统计;任务系统无 result 字段,结果另行落库 |
| 查询执行位置 | Rust 主进程直连 tiberius | 凭据只存在于 Rust keyring;安全设计规定 compute-worker 永不接触凭据 |
| 结果转数据集 | 结果写缓存 CSV,复用 `previewSteelDataset`/`saveSteelDataset` 文件管道 | 零新数据集代码,保留文件哈希溯源,列映射 UI 复用 |

## 架构

```
一级导航: 工作台 / 对话 / 知识库 / 数据库★ / 数据分析 / 扩展
设置页:   标签导航(提供商 | 通用 | 权限 | 数据库)
            └─ 数据库标签 = 增强版 DatabaseConnectionsPanel

DatabasePage
  └─ submit_database_query(后台任务)
       └─ database_query handler
            ├─ query.rs 安全包装(guard)
            ├─ tiberius 执行(SELECT TOP (n) 包装,超时,可取消)
            ├─ 结果 JSON -> database_query_results 表(SQLite)
            └─ 结果 CSV  -> app-data 缓存文件
  └─ get_database_query_result(task_id) -> 前端结果表格
  └─ 「送入分析」: saveSteelDataset({sourcePath: 缓存CSV}) -> 跳转分析页
```

## Rust 侧设计

### `src/database/` 模块扩展

- `query.rs` -- 查询安全包装器:
  - 校验:trim 后去除尾部分号,必须为单条语句,首关键字(不区分大小写)必须是 `SELECT` 或 `WITH`;否则拒绝并返回明确错误。
  - 包装:外层强制 `SELECT TOP (n) * FROM ( <用户SQL> ) AS [_bloomery_q]`,使 INSERT/UPDATE/DELETE/DDL/多语句在结构上不可能执行。
  - 行数上限:默认 500,UI 可选,硬上限 5000。
  - 超时:沿用连接记录的 `timeout_ms`(既有 clamp 1s-60s)。
- `catalog.rs` -- 目录查询:
  - `list_databases`:`SELECT name FROM sys.databases ORDER BY name`。
  - `list_database_tables(database: Option<String>)`:带库名时先对库名做 `[]` 标识符转义(反注入),再 `USE [db]` 后查 `sys.tables`/`sys.schemas`;不带时保持现有默认库行为。
- 取消:handler 内 `tokio::select!` 轮询 `cancellation_requested()`,取消即 drop tiberius 查询 future。

### 后台任务

- 新 kind:`database_query`,注册为 scheduler 第 10 个 handler。
- payload:`{connection_id, database, sql, row_limit}`。
- 执行前校验:连接存在、属于当前 workspace、`enabled = true`、密码已配置。
- 结果双写:
  1. SQLite 新表 `database_query_results`(迁移 0024):`id`(=task_id)、workspace_id、connection_id、query_text、database、row_count、truncated、duration_ms、columns_json、rows_json、created_at。
  2. app-data 缓存 CSV(如 `query-cache/<task_id>.csv`),文件头注释记录来源(连接 id、查询文本、执行时间)。
- 失败错误码:`connection_failed` / `query_guard_rejected` / `query_failed` / `result_write_failed`。

### 新增/增强 Tauri command

- `list_databases(connection_id)` -> `Vec<String>`
- `list_database_tables(connection_id, database?)` -> `Vec<String>`(增强现有命令)
- `submit_database_query(payload)` -> 任务 id
- `get_database_query_result(task_id)` -> `{columns, rows, row_count, truncated, duration_ms, csv_path}`
- `list_database_query_results()` -> 最近查询摘要(倒序,供历史列表)
- `test_database_connection(id)` 增强:测完把 `last_checked_at` / `last_latency_ms` / `last_version` / `last_error` 写回连接记录(列由迁移 0024 增加;错误也记录,便于离线时显示原因)。
- `enabled` 生效:`submit_database_query`、`list_databases`、`list_database_tables` 均拒绝 `enabled = false` 的连接。

### 迁移 0024

1. `database_connections` 增加健康列:`last_checked_at TEXT`、`last_latency_ms INTEGER`、`last_version TEXT`、`last_error TEXT`(均可空)。
2. 新建 `database_query_results` 表(workspace 作用域,按 `created_at DESC` 索引)。

## 前端侧设计

### 一级导航

- `navigation.ts`:`SectionId` 增加 `"databases"`,插入 `primaryNavigationSections` 知识库之后;图标 `Database`;新增 i18n key(zh/en)。
- `BloomeryApp.tsx`:渲染 `DatabasePage`(使用通用壳,与知识库/分析一致)。

### `features/databases/DatabasePage.tsx`(新)

- 顶部工具条:连接选择器(仅 enabled 连接;无连接时引导去设置)+ 库选择器(sys.databases)+ 刷新。
- 左侧:表浏览树(schema.table 列表,点击表名把 `SELECT TOP (500) * FROM [schema].[table]` 填入编辑器)。
- 中部:SQL 编辑器(textarea,等宽字体)+ 行数上限选择(100/500/1000/5000)+ 运行/取消按钮;运行后任务进度复用既有任务模式;结果到达后渲染结果表格 + `truncated` 提示。
- 结果区动作:「送入数据分析」--调 `saveSteelDataset({sourcePath: csv_path})`,成功后切到分析分区并激活该数据集;失败时展示原始错误。
- 查询历史:最近 10 条(来自 `list_database_query_results`),点击回填编辑器。
- 状态管理沿用项目惯例:页面内 controller hook + bridge 调用,不引入状态库。

### 设置页标签导航

- `SettingsPage.tsx` 内部 `useState` + `role="tablist"/"tab"/"tabpanel"`:提供商(Provider 卡片 + 套餐)/ 通用(主题、语言、安全提示)/ 权限(PermissionRulesPanel)/ 数据库(DatabaseConnectionsPanel)。
- 页面头部(标题、诊断、刷新)保持在标签上方。
- DatabaseConnectionsPanel 增强:健康徽标(上次检测时间/延迟/版本或错误,来自新健康列)、enabled 开关、timeout_ms 编辑框、重复 host+port+username 提示。

## 错误处理

- 连接失败/登录失败/超时:沿用现有中文友好错误;超时带毫秒数。
- 查询被 guard 拒绝:返回具体原因(非 SELECT、多语句等)。
- SQL 执行错误:透传 SQL Server 原始错误信息。
- 截断:结果 `truncated = true`,UI 显示「已达到行数上限 N」。
- CSV 写失败:任务失败,错误码 `result_write_failed`。

## 测试策略(全程 TDD,先失败测试后实现)

- Rust 单测:query guard(拒绝 INSERT/UPDATE/DELETE/DDL/多语句/分号变体/前导注释伪装;允许 SELECT/WITH/ORDER BY/中文表名)、catalog 标识符转义、repository(健康列读写、query_results 表 CRUD、workspace 隔离)、handler 落库路径(guard 拒绝路径 + 结果写入)。
- 前端 Vitest:DatabasePage 地标/连接选择/提交任务/取消/结果渲染/truncated/送入分析/历史回填;设置页标签切换与内容归属;导航新分区。
- 不做:真连 SQL Server 的自动化测试(由 `test_database_connection` 手动验证);性能基准。

## 明确不做(YAGNI)

查询历史全文搜索、SQL 自动补全、语法高亮、虚拟滚动、跨库 JOIN、结果图表、Windows 集成认证、TLS 显式配置、查询结果直插 steel_datasets 表。

## 涉及文件清单(预估)

- Rust:`src/database/{mod,query,catalog}.rs`、`src/tasks/`(注册 handler)、`src/app/database_commands/`、`src/storage/migrations/0024_*.sql` + `migrations.rs`、`src/storage/repositories/database_connections.rs`、新增 `repositories/database_query_results.rs`、`src/app/commands.rs`、`tests/`(新增 query guard / results 仓储测试)
- 前端:`src/app/{navigation,BloomeryApp}.tsx`、`src/features/databases/`(新)、`src/features/settings/{SettingsPage,DatabaseConnectionsPanel}.tsx`、`src/bridge/desktop.ts`、`src/i18n/locale.tsx`、`src/design/polish.css`
