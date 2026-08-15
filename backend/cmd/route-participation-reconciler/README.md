# Route Participation Reconciler

该 Worker 将 Growth 持有的路线参与期望状态收敛到 BBS。Gateway 的同步 BBS 调用是低延迟快路径；即使同步调用超时、响应丢失或进程在双写之间退出，Worker 仍会继续补偿。

## 一致性模型

- Growth 在创建来源 Journey 的同一 PostgreSQL 事务内写入 `route_participation_intents`。
- 每个 `(user_id, route_id)` 只有一条期望状态；加入、退出和重入仅在状态变化时递增版本。
- Worker 使用 `FOR UPDATE SKIP LOCKED` 和租约横向扩容，并发调用 BBS。
- BBS 在参与事实行保存 `last_intent_version`，低版本命令会被原子忽略，因此延迟请求和 Worker 崩溃不会覆盖新状态。
- 失败任务使用带抖动的指数退避持续重试；新版本会清除旧租约并立即进入待处理状态。

## 启动

先执行 `0015_route_participation_reconciliation.sql`，并确保 BBS gRPC 已可用：

```bash
DATABASE_URL=postgres://bookway:bookway-local-only@127.0.0.1:5432/bookway \
BBS_GRPC_URL=http://127.0.0.1:18002 \
cargo run -p bookway-route-participation-reconciler
```

可调参数：

| 环境变量 | 默认值 | 范围 | 含义 |
| --- | ---: | ---: | --- |
| `ROUTE_RECONCILE_BATCH_SIZE` | 100 | 1-1000 | 每轮领取数量 |
| `ROUTE_RECONCILE_CONCURRENCY` | 16 | 1-128 | 单实例并发数 |
| `ROUTE_RECONCILE_LEASE_SECONDS` | 30 | 5-300 | 崩溃后重新领取等待时间 |
| `ROUTE_RECONCILE_REQUEST_TIMEOUT_MS` | 3000 | 100-30000 | 单次 BBS 调用预算 |

生产监控至少应覆盖待处理数量、最老待处理时长、每分钟失败数、`attempts` 高分位和 BBS gRPC 延迟。待处理条件为 `applied_version < version`；`last_error` 仅用于诊断，不会让任务永久停止重试。
