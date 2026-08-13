# User Event 用户行为接收服务

## 职责

负责接收 App 上报的曝光、点击、播放、互动和搜索行为。它校验批次与事件字段，以 `event_id` 幂等去重，并返回接收、重复和拒绝数量。用户身份只信任 Gateway 写入的 `x-user-id`，不接受请求体伪造身份。

## 接口

- `POST /v1/events`：接收最多 100 条事件。
- 内部 gRPC：`ingest`。
- `GET /health`：健康检查。

首版支持 `impression`、`click`、`view`、`like`、`bookmark`、`share`、`hide`、`complete`、`follow` 和 `search_submit`。`session_id`、`component_id`、`occurred_at`、`source` 和 `event_id` 必填；实体 ID 使用 UUID，事件时间使用 RFC 3339，单个请求体上限为 256 KiB。

## 环境变量

`USER_EVENT_ADDR` 和 `USER_EVENT_GRPC_ADDR`，默认分别监听 `127.0.0.1:8089`、`127.0.0.1:18089`。

## 数据与生产化

默认内存 Repository 用于无依赖联调；`STORAGE_MODE=postgres` 时，事件以 `event_id` 幂等写入 PostgreSQL，并在同一事务写入 Outbox。`bookway-outbox-relay` 使用 `FOR UPDATE SKIP LOCKED` 认领事件，发布到 Kafka/Redpanda，失败指数退避，10 次后进入 `dead`。下一阶段补齐 Schema 管理、按用户分区的幂等消费者、ClickHouse/湖仓、死信重放、过载保护、采样与脱敏策略。
