# 申诉通知调度器

`bookway-appeal-notification-dispatcher` 是常驻 Worker，不提供 HTTP 或 gRPC 监听端口。它领取 `content_appeal_notification_jobs` 中的终态申诉任务；获准恢复的内容会先由它幂等恢复公开，再以 Growth 的 `(kind, source_id)` 唯一约束写入作者私有收件箱。

## 投递语义

- `content-audit` 在保存不可变的 `resolved` 或 `rejected` 决定时，与任务在同一 PostgreSQL 事务中创建记录；审核成功不依赖 Growth 在线。
- Worker 通过 `FOR UPDATE SKIP LOCKED` 领取工作并持有租约。失败、超时和租约过期会重新进入可领取状态，退避为指数级并带少量随机抖动；第 10 次失败后标记为 `dead`。
- `restore_content` 决定会带 `x-service-token` 调用幂等的 `bbs-link.restore`，再调用 `bbs-link.get_public` 读回确认。内容尚未公开时不会通知作者，任务保持可重试；Worker 在恢复调用或读回前崩溃时，下次领取会安全重放恢复。
- 稳定来源键为 `content-appeal:{appeal_id}:{resolved|rejected}`。即使 Worker 在 Growth 成功写入后、确认任务前崩溃，重复投递也只会保留一个收件箱项。

## 运行

先完成 `0026_content_appeal_notification_jobs.sql` 迁移，并确保 PostgreSQL、`bbs-link` 和 `growth` 已就绪：

```bash
cargo run -p bookway-appeal-notification-dispatcher
```

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DATABASE_URL` | - | 任务表所在 PostgreSQL 连接串，必填 |
| `BBS_LINK_GRPC_URL` | `http://127.0.0.1:18004` | 恢复公开写入与读取确认接口 |
| `GROWTH_GRPC_URL` | `http://127.0.0.1:8081` | 通知收件箱写入接口 |
| `SERVICE_AUTH_REQUIRED` | `false` | 为 `true` 时必须提供有效的服务令牌 |
| `SERVICE_AUTH_TOKEN` | - | `bbs-link.restore` 和公开读取确认的内部服务令牌；启用服务鉴权时必填 |
| `APPEAL_NOTIFICATION_BATCH_SIZE` | `100` | 单次领取上限，范围 1-1000 |
| `APPEAL_NOTIFICATION_CONCURRENCY` | `16` | 单批并行投递数，范围 1-128 |
| `APPEAL_NOTIFICATION_LEASE_SECONDS` | `30` | 领取租约，范围 5-300 秒 |
| `APPEAL_NOTIFICATION_REQUEST_TIMEOUT_MS` | `3000` | 单次投递超时，范围 100-30000 毫秒 |

生产环境应监控待处理任务年龄、失败次数和 `delivery_status = 'dead'` 的数量；`dead` 任务需要先排除目标服务或数据问题，再通过受控运维流程重置为 `pending`。不要删除终态审核决定或对应任务来进行补偿。
