# Bloomery

Bloomery 是一个本地优先的钢铁材料研发智能体桌面应用，基于 Tauri 2 + Rust + React 构建。对话、历史记录、个人记忆和本地模型配置全部保存在本机 SQLite 中；云端能力（知识检索、模型训练、工艺寻优、文献解析）是可插拔的——不配置云端服务时，本地功能依然完整可用。

> Bloomery（块炼炉）是人类最早的炼铁装置，以小而自足的方式将矿石炼成铁——正如这个应用以本地优先的方式运行智能体。

## 特性

- **本地优先**：Rust 本地智能体负责意图识别、上下文组装和 LLM 流式回答；对话与记忆保存在本机 `bloomery` SQLite 数据库中，不依赖任何服务器。
- **本地 LLM 配置**：直接连接任意 OpenAI 兼容 API（Ollama、vLLM、DeepSeek、通义等），API Key 只保存在本机。
- **云端能力可插拔**：在设置中填写云端 API 地址后，可选择启用知识库检索、模型训练、成分/工艺寻优、文献 OCR 解析等云端长任务；云任务经过路径与方法白名单校验，并镜像到本地任务列表。
- **富文本体验**：Markdown、KaTeX 数学公式、代码高亮、docx/PDF 预览、图表可视化。

## 目录结构

```
bloomery/
├── frontend/     # React 18 + Vite + Tailwind 前端（含 src/desktop/ 桌面适配层）
└── src-tauri/    # Tauri 2 / Rust 宿主：本地智能体、SQLite、云任务代理
```

## 快速开始

依赖：Node.js 20/22/24、Rust（stable）、[Tauri 2 前置依赖](https://tauri.app/start/prerequisites/)。

```bash
# 安装前端依赖
cd frontend
npm install

# 开发模式（自动启动前端 + 桌面窗口）
cd ../src-tauri
cargo tauri dev

# 构建发行版
cargo tauri build
```

Windows 用户可以直接双击根目录的 `start-desktop.bat`，它会自动安装缺失的依赖并进入开发模式。

## 配置

| 配置项 | 位置 | 说明 |
| --- | --- | --- |
| 本地 LLM | 应用内「设置 → 模型」 | OpenAI 兼容 base_url / model / API Key，仅存本机 |
| 云端 API 地址 | 应用内「设置」 | 留空即纯本地模式；填写后启用云任务能力 |
| 数据库 | 系统应用数据目录 | SQLite 单文件，按用户隔离 |

## 开发

```bash
# Rust 检查与测试
cd src-tauri
cargo check
cargo test

# 前端类型检查 + 构建
cd frontend
npm run build
```

## License

[MIT](./LICENSE)
