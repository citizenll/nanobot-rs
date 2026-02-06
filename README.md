# nanobot-rs

`nanobot` 的 Rust 移植版（首版）。

## 已移植模块

- 配置系统：`~/.nanobot/config.json` 读写、`camelCase` 序列化、基础迁移逻辑
- 消息总线：`InboundMessage` / `OutboundMessage` / async 队列
- 会话管理：JSONL 持久化会话
- Memory：`memory/MEMORY.md` 与当日日志读取
- Agent 核心循环：LLM 调用 + tool call 执行闭环
- Provider：OpenAI-compatible Chat Completions 接口
- 工具系统：
  - `read_file`
  - `write_file`
  - `edit_file`
  - `list_dir`
  - `exec`
  - `web_search`（Brave API）
  - `web_fetch`
  - `message`
- 参数校验：对齐原 Python 的 schema 校验语义（含测试）

## 当前未移植

- Telegram / Discord / WhatsApp / Feishu channel 适配层
- Cron 服务与 heartbeat
- 子代理（spawn）
- 技能元数据解析与自动装载（当前仅保留基础系统提示拼装）

## 快速开始

```bash
cargo run -- onboard
```

编辑配置 `~/.nanobot/config.json`，至少设置一个可用 API Key，例如：

```json
{
  "providers": {
    "openai": {
      "apiKey": "sk-xxx"
    }
  },
  "agents": {
    "defaults": {
      "model": "gpt-4o-mini"
    }
  }
}
```

单次对话：

```bash
cargo run -- agent -m "What is 2+2?"
```

交互模式：

```bash
cargo run -- agent
```

查看状态：

```bash
cargo run -- status
```

## 开发

```bash
cargo test
```
