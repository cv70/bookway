# User Event 用户行为接收服务

## 职责

负责接收 App 上报的曝光、点击、播放、互动和搜索行为。它校验批次与事件字段，以 `event_id` 幂等去重，并返回接收、重复和拒绝数量。用户身份只信任 Gateway 写入的 `x-user-id`，不接受请求体伪造身份。带 `request_id` 的事件必须同时带内容 ID、位置和明确来源；服务按来源通过 Recommend Main 或 Search Main 批量核对用户、会话、曝光请求、内容和位置，伪造或错位的归因事件会被拒绝。相应归因服务暂时不可用时，事件仍会写入但移除归因字段，避免把不可验证数据送入训练链路。

## 接口

- `POST /v1/events`：接收最多 100 条事件。
- 内部 gRPC：`ingest`。
- `GET /health`：健康检查。

首版支持 `impression`、`click`、`view`、`like`、`bookmark`、`save_knowledge`、`share`、`hide`、`complete`、`join_route`、`follow`、`report`、`search_submit` 和 `purchase`。`session_id`、`component_id`、`occurred_at`、`source` 和 `event_id` 必填；实体 ID 使用 UUID，事件时间使用 RFC 3339，单个请求体上限为 256 KiB。活动的 `like`、`bookmark` 与 `hide` 由 Gateway 在 Interaction Status 成功提交后以用户、内容和反应类型的稳定 UUID 写入；路线加入和知识收集同样由 Gateway 生成稳定事件。若这些动作来自被服务的 Feed 或搜索结果，Gateway 会将客户端会话、曝光请求和位置透传至本服务，仍需逐批核验才保留归因；重复请求与丢失响应不会放大为多条偏好，失活反应不产生新的信号。`purchase` 只能由 `mall-order` 在受服务令牌保护的支付确认后写入，并使用订单稳定键幂等归因到公共路线；其事件链路降级不会回滚已支付订单。`save_knowledge` 权重高于普通收藏但低于实际加入/完成路线；它不会同时生成 `bookmark`，避免一次收集被双重计数。`join_route` 在 Gateway 确认 Growth 与 BBS 的加入状态后以用户和路线稳定去重，避免客户端丢失响应或重复点击把一次真实采用放大为多次偏好。`hide` 可选携带受限的 `negative_feedback_reason`：`not_relevant` 会降低同领域探索，`already_seen` 只降低重复内容，`low_quality` 会降低相应创作者亲和度；Gateway 只从成功写入的隐藏状态派生该原因，其他反应携带该字段会在 REST 边界被拒绝。

`complete` 既可以描述普通客户端完成反馈，也可以由 Gateway 在 Growth 已持久化采用路线行动后以固定 UUID 写入。后者的 `content_id` 是公共路线而不是私有 Action ID，能安全参与“发现 → 加入 → 执行”的推荐特征；没有来源路线的私人行动不会生成该服务端信号。

`gateway-*` 来源保留给 Gateway 的服务端动作事件。公开 Gateway 事件入口会拒绝客户端携带这些来源，避免伪造路线加入或完成信号污染特征；内部 gRPC 调用仍由服务令牌保护。

## 环境变量

`USER_EVENT_ADDR` 和 `USER_EVENT_GRPC_ADDR`，默认分别监听 `127.0.0.1:8089`、`127.0.0.1:18089`。`RECOMMEND_MAIN_GRPC_URL` 默认 `http://127.0.0.1:8083`，用于内部归因核验；在 `SERVICE_AUTH_REQUIRED=true` 下会携带服务令牌。

设置 `REDIS_URL` 后，任一新事件在 PostgreSQL 成功提交后都会删除该用户的 `bookway:features:{user_id}` 缓存，使下一次推荐重新派生在线特征。Redis 不可用只记录告警，绝不会阻塞事件落库或 Outbox 投递。

## 数据与生产化

默认内存 Dao 用于无依赖联调；`STORAGE_MODE=postgres` 时，事件以 `event_id` 幂等写入 PostgreSQL，并在同一事务写入 Outbox。`bookway-outbox-relay` 使用 `FOR UPDATE SKIP LOCKED` 认领事件，发布到 Kafka/Redpanda，失败指数退避，10 次后进入 `dead`。`feature-snapshot` Job 从这条已验证事件链生成带版本、窗口、TTL 和血缘的用户特征快照；后续补齐 Schema 管理、按用户分区的幂等消费者、ClickHouse/湖仓、死信重放、过载保护、采样与脱敏策略。
