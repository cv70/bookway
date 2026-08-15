# 社区通知调度器

`bookway-community-notification-dispatcher` 是常驻 Worker，不提供 HTTP 或 gRPC 监听端口。它从 `community_notification_jobs` 领取由 Gateway 在点赞、评论、回复和关注成功后创建的工作项，并直接调用 Growth 生成的 `CreateNotification` Client。

## 投递语义

- Worker 使用 `FOR UPDATE SKIP LOCKED` 和租约领取任务；超时、服务不可用和进程崩溃会重新领取，指数退避后最多尝试 10 次，随后标记为 `dead`。
- `source_id` 与 Growth 的 `(kind, source_id)` 唯一约束一致。Growth 已成功但 Worker 在确认前崩溃时，重放只会保留一个收件箱项。
- 任务格式错误会直接进入 `dead`；服务认证、网络和 Growth 错误会重试。生产环境必须对队列最老待处理时间、`attempts` 和 `status = 'dead'` 数量告警。
- Gateway 无法与点赞、评论或关注服务开启同一事务，因此仍存在互动写入成功、Gateway 在创建任务前崩溃的跨服务窗口；这不是此队列能够伪装为原子完成的事情。

## 运行

先执行 `0045_community_notification_jobs.sql`，再在 PostgreSQL 与 Growth 都可用时启动：

```bash
cargo run -p bookway-community-notification-dispatcher
```

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DATABASE_URL` | - | 任务表所在 PostgreSQL 连接串，必填 |
| `GROWTH_GRPC_URL` | `http://127.0.0.1:8081` | Growth 通知收件箱接口 |
| `SERVICE_AUTH_REQUIRED` | `false` | 为 `true` 时必须配置有效的 `SERVICE_AUTH_TOKEN` |
| `SERVICE_AUTH_TOKEN` | - | Growth 内部 gRPC 服务令牌 |
| `COMMUNITY_NOTIFICATION_BATCH_SIZE` | `100` | 单次领取上限，范围 1-1000 |
| `COMMUNITY_NOTIFICATION_LEASE_SECONDS` | `30` | 领取租约，范围 5-300 秒 |
| `COMMUNITY_NOTIFICATION_MAX_ATTEMPTS` | `10` | 最大尝试次数，范围 1-100 |
| `COMMUNITY_NOTIFICATION_CONCURRENCY` | `16` | 单批并行投递数，范围 1-128 |
| `COMMUNITY_NOTIFICATION_REQUEST_TIMEOUT_MS` | `3000` | 单次 Growth 调用超时，范围 100-30000 毫秒 |
