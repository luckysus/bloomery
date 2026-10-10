# Suna UI/UX 统一设计规范

## 视觉

以用户提供的 UI 图为基准：浅蓝白科研风、白色卡片、深蓝文字、Suna
蓝主操作、细浅蓝边框、轻阴影、适度圆角、充足留白。避免黑紫赛博朋克、霓虹、游戏化和大面积强渐变。优先复用仓库
Design Token；若没有则集中定义，禁止组件内散落硬编码颜色。

建议颜色：Primary `#1677ff`、主文字 `#102a72`、次级文字 `#536b9b`、边框
`#dcecff`、背景 `#f6faff`、表面 `#ffffff`。

## 主布局

桌面三栏：左侧 Sidebar（品牌、导航、最近会话、用户入口）；中间 Main
Workspace；右侧 Agent Workspace（当前任务、Agent
状态、工具调用、进度，可折叠）。窗口变窄时允许折叠右栏，不得重叠。

## 统一组件

AppShell、Sidebar、TopBar、PageHeader、AgentWorkspace、StatusBadge、EmptyState、ErrorState、LoadingState、ConfirmDialog、SearchBar、DataTable、FileDropzone、ProgressTimeline、ModelSelector、MarkdownAnswer、CitationList、TaskControls。

## 页面

主界面、对话中心、知识中心、文献研究、数据实验室、性能预测、工艺优化、实验助手、Agent
管理、工具中心、MCP 管理、Skill 管理、模型管理、设置中心、关于/诊断。

## 截图还原

用户提供的截图是布局与视觉参考，不代表示例数据是真实数据。尽量还原结构、比例、间距、颜色、图标和排版；实际状态必须来自
API，示例数据要明确标记。完成后提供截图和偏差说明。

## 所有页面状态

必须覆盖 loading、empty、success、error、retry、validation error、danger
confirmation 和长任务取消。禁止用固定百分比、随机状态或 `setTimeout`
伪造任务进度。

## 可访问性

图标按钮需有
aria-label；键盘焦点清晰；颜色不是唯一状态表达方式；表格有明确列名；主题切换后文字对比度仍足够。
