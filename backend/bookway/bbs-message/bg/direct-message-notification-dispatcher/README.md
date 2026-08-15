# 私信通知调度器

`bookway-direct-message-notification-dispatcher` 是 `bbs-message` 的常驻后台任务。服务在同一 PostgreSQL 事务中写入私信和 `direct_message_notification_jobs`；Worker 使用 Growth 的生成 `CreateNotification` Client 将待投递消息放入接收者收件箱。

## 投递语义

- `message_id` 是 Outbox 主键，Growth 使用 `community + direct-message:{message_id}` 作为幂等来源键。Growth 已成功而 Worker 确认前崩溃时，重放不会创建重复收件箱项。
- Worker 使用 `FOR UPDATE SKIP LOCKED`、租约和指数退避领取任务。网络、认证或 Growth 暂时不可用会重试；达到最大尝试次数后标记 `dead`，需要告警和人工处置。
- 通知只包含会话、消息和发送者的导航 ID；正文固定为“打开会话查看详情”，绝不会将私信正文放入收件箱、日志或下游推送负载。
- 这是消息服务本地事务的一部分，因此不存在“私信已持久化但服务在创建通知任务前崩溃”的跨服务窗口。`memory` 模式仅用于本地开发，不运行此 PostgreSQL Worker。

## 运行

先执行 `0056_bbs_message_notification_delivery.sql`，再在 PostgreSQL 与 Growth 可用时启动：

```bash
cargo run -p bookway-direct-message-notification-dispatcher
```

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DATABASE_URL` | - | 任务表所在 PostgreSQL 连接串，必填 |
| `GROWTH_GRPC_URL` | `http://127.0.0.1:8081` | Growth 通知收件箱接口 |
| `SERVICE_AUTH_REQUIRED` | `false` | 为 `true` 时必须配置有效的 `SERVICE_AUTH_TOKEN` |
| `SERVICE_AUTH_TOKEN` | - | Growth 内部 gRPC 服务令牌 |
| `DIRECT_MESSAGE_NOTIFICATION_BATCH_SIZE` | `100` | 单次领取上限，范围 1-1000 |
| `DIRECT_MESSAGE_NOTIFICATION_LEASE_SECONDS` | `30` | 领取租约，范围 5-300 秒 |
| `DIRECT_MESSAGE_NOTIFICATION_MAX_ATTEMPTS` | `10` | 最大尝试次数，范围 1-100 |
| `DIRECT_MESSAGE_NOTIFICATION_CONCURRENCY` | `16` | 单批并行投递数，范围 1-128 |
| `DIRECT_MESSAGE_NOTIFICATION_REQUEST_TIMEOUT_MS` | `3000` | 单次 Growth 调用超时，范围 100-30000 毫秒 |
