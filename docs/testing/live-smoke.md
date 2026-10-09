# 真实环境联调

Suna 保留了一个可编译但默认跳过的真实环境 smoke test，覆盖聊天 Provider、PostgreSQL、pgvector、Embedding 和 Reranker。它不会创建、删除或迁移数据库，只执行连接、扩展、知识表和最小请求检查。

在 PowerShell 中设置环境变量后运行：

```powershell
$env:SUNA_LIVE_SMOKE = "1"
$env:SUNA_LIVE_CHAT_PROVIDER = "open_ai_compatible"
$env:SUNA_LIVE_CHAT_BASE_URL = "https://api.example.com/v1"
$env:SUNA_LIVE_CHAT_MODEL = "your-chat-model"
$env:SUNA_LIVE_CHAT_API_KEY = "your-chat-key"
$env:SUNA_LIVE_POSTGRES_URL = "postgres://user:password@127.0.0.1:5432/suna"
$env:SUNA_LIVE_SILICONFLOW_BASE_URL = "https://api.siliconflow.cn/v1"
$env:SUNA_LIVE_SILICONFLOW_API_KEY = "your-siliconflow-key"
$env:SUNA_LIVE_EMBEDDING_MODEL = "BAAI/bge-m3"
$env:SUNA_LIVE_RERANK_MODEL = "BAAI/bge-reranker-v2-m3"
cargo test --test live_smoke -- --nocapture
```

测试只输出断言结果，不会把密钥或连接串写入日志。CI 不设置 `SUNA_LIVE_SMOKE=1` 时，测试会在发起网络请求前返回。
