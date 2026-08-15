# 举报下架调度器

`bookway-content-report-restriction-dispatcher` 是常驻 Worker，不提供 HTTP 或 gRPC 监听端口。它领取 `content_report_restriction_jobs` 中已接受举报的下架任务，带内部服务令牌调用幂等的 `bbs-link.restrict`，再确认公开读取已经不可用。

## 投递语义

- `content-audit` 在同一 PostgreSQL 事务内保存 `resolved + restrict_content` 审核决定和任务；审核成功不依赖 `bbs-link` 在线。
- Worker 使用 `FOR UPDATE SKIP LOCKED` 与租约领取任务。失败、超时或租约过期都会指数退避重试，并有少量随机抖动；第 10 次失败后标记为 `dead`。
- `restrict` 已经成功、但公开读取尚未消失时，Worker 不会将任务标记完成；下次领取会幂等重放下架并再次确认。因此 Gateway 或 Worker 在调用间崩溃不会重新暴露内容。

## 运行

先完成 `0027_content_report_restriction_jobs.sql` 迁移，并确保 PostgreSQL 与 `bbs-link` 已就绪：

```bash
cargo run -p bookway-content-report-restriction-dispatcher
```

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DATABASE_URL` | - | 任务表所在 PostgreSQL 连接串，必填 |
| `BBS_LINK_GRPC_URL` | `http://127.0.0.1:18004` | 下架写入与公开读取确认接口 |
| `SERVICE_AUTH_REQUIRED` | `false` | 为 `true` 时必须提供有效的服务令牌 |
| `SERVICE_AUTH_TOKEN` | - | `bbs-link.restrict` 和公开读取确认的内部服务令牌；启用服务鉴权时必填 |
| `REPORT_RESTRICTION_BATCH_SIZE` | `100` | 单次领取上限，范围 1-1000 |
| `REPORT_RESTRICTION_CONCURRENCY` | `16` | 单批并行执行数，范围 1-128 |
| `REPORT_RESTRICTION_LEASE_SECONDS` | `30` | 领取租约，范围 5-300 秒 |
| `REPORT_RESTRICTION_REQUEST_TIMEOUT_MS` | `3000` | 单次执行超时，范围 100-30000 毫秒 |

生产环境应监控待处理任务年龄、失败次数和 `delivery_status = 'dead'` 的数量。处理 `dead` 任务前先排除内容服务或数据异常，再通过受控运维流程将任务重置为 `pending`；不要删除原举报决定或任务。
