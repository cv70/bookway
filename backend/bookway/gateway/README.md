# Gateway 网关服务

## 职责

`gateway` 是移动端唯一访问入口，负责对外 API 版本、CORS、请求聚合以及上游传输错误和领域错误的统一转换。该服务不持有业务数据。

## 对外接口

- `GET /v1/feed`：推荐流。
- `GET /v1/search`、`GET /v1/search/suggestions`：搜索与联想词。
- `POST /v1/events`：批量上报曝光、点击等用户行为。
- `POST /v1/media/upload-url`、`POST /v1/media/{id}/complete`：对象存储直传控制面。
- `GET /v1/media/{id}`：媒体元数据与 CDN 地址。
- `/v1/journeys`、`/v1/today`、`/v1/actions/*`：路线与行动。
- `GET /v1/notifications`、`PATCH /v1/notifications/{notification_id}/read`：用户私有通知收件箱与已读确认。
- `/v1/posts/*`：内容、评论和互动。
- `GET /v1/me/posts`、`GET /v1/me/appeals`：当前作者私有的内容状态与申诉历史，支持状态筛选和游标续页。
- `GET /v1/moderation/reports`、`PATCH /v1/moderation/reports/{report_id}`：受限的举报队列与人工处置接口。
- `POST /v1/posts/{id}/appeals`：受限内容作者提交申诉；`GET/PATCH /v1/moderation/appeals*`：审核员处理申诉与恢复决定。
- `/v1/users/*`：关注、拉黑和静音关系。
- `GET /v1/route-participations`、`POST /v1/routes/{route_id}/join`、`PUT /v1/routes/{route_id}/participation`：恢复、加入和退出公共路线。

`GET /v1/posts/{id}`、路线读取以及点赞、评论、举报前的内容校验都使用 `bbs-link.get_public`；草稿、审核中和受限内容不会经 Gateway 暴露或接收公开互动。详情读取、点赞、评论、举报和路线加入还会执行同一查看者的社交可见性检查，直接内容 ID 不能绕过拉黑或静音。退出已加入路线不依赖内容可见性，避免关系变更后无法退出。评论作者可通过 `DELETE /v1/posts/{post_id}/comments/{comment_id}` 删除自己的历史评论；这条撤回路径由评论服务按可信身份原子校验所有权，不因之后的社交关系变化而失去撤回能力。点赞、评论和关注成功后会以稳定来源键尽力写入对方的社区收件箱通知；公开回复还会分别通知帖子作者与父评论作者，后者与前者相同或属于回复者时只保留一条或不投递。点赞自己内容、评论自己内容和关注自己不会产生通知，通知服务故障也不会影响已成功的互动。事件上报由 Gateway 注入可信用户身份后批量转发给 `user-event`。

`GET /v1/search` 与 `GET /v1/search/suggestions` 会从当前可信身份读取受服务令牌保护的 BBS 可见性策略，合并拉黑/静音作者及拉黑当前用户的作者，并将规范化集合传给内部搜索链路；策略不可用时请求失败而不会以未过滤结果降级。客户端提供的同名查询字段会被 Gateway 覆盖。搜索游标绑定查看者和可见性策略，用户改变拉黑或静音关系后应重新搜索；内容派生的联想词遵循同一策略。

`AUTH_REQUIRED=true` 时 Gateway 校验 HS256 Bearer JWT，并只从已验证的 `sub` 与 `roles` 写入内部 `x-user-id`、`x-user-roles`；来路请求携带的同名头会先被清除。`/v1/me/*` 的作者过滤也只能来自该可信身份，不能由 query string 指定。审核接口还要求角色为 `moderator`、`trust_safety` 或 `admin`，且本地关闭鉴权时也一律拒绝。`resolved + restrict_content` 会通过带 `x-service-token` 的内部调用将内容转为 `restricted`；获准申诉的 `resolved + restore_content` 只能恢复原本受限的内容。Gateway 对两个动作都只尝试一次低延迟快路径，失败只记录告警，绝不把已持久化的审核决定改写为失败；报告下架与获准申诉恢复分别由独立调度器携带服务令牌重试，前者确认公开读取不可用，后者确认公开读取可用后再投递终态通知。

路线加入先由 Growth 事务性创建或复用私人 Journey 并记录参与意图，再同步写入 BBS；退出先更新最新意图再写 BBS。同步调用失败时由 `bookway-route-participation-reconciler` 自动重试，BBS 会拒绝低于当前版本的延迟命令。

## 依赖

`growth`、`bbs-feed`、`search-main`、`user-event`、`bbs-link`、`bbs`、`comment`、`commonlikestatus`、`media`、`content-audit`。

## 环境变量

`GATEWAY_ADDR`、`GROWTH_GRPC_URL`、`BBS_FEED_GRPC_URL`、`BBS_LINK_GRPC_URL`、`SEARCH_MAIN_GRPC_URL`、`USER_EVENT_GRPC_URL`、`BBS_GRPC_URL`、`COMMENT_GRPC_URL`、`LIKE_STATUS_GRPC_URL`、`MEDIA_GRPC_URL`、`CONTENT_AUDIT_GRPC_URL`、`AUTH_REQUIRED`、`AUTH_JWT_SECRET`、`SERVICE_AUTH_TOKEN`、`HTTP_CONNECT_TIMEOUT_MS`、`HTTP_REQUEST_TIMEOUT_MS`、`REDIS_URL`、`REDIS_CONNECT_TIMEOUT_MS`、`REDIS_COMMAND_TIMEOUT_MS`、`RATE_LIMIT_PER_MINUTE`。审核 JWT 的 `roles` 声明是字符串数组；角色由身份系统签发，不能由客户端请求头提供。

## 生产化待办

当前已接入 JWT、服务令牌、请求 ID、Redis 限流和统一调用超时。下一阶段补齐 OIDC/JWKS 与密钥轮换、接口级限流策略、熔断、OpenTelemetry 上下文传播、OpenAPI 契约和分接口容量压测。
