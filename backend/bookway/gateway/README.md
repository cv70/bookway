# Gateway 网关服务

## 职责

`gateway` 是移动端唯一访问入口，负责对外 API 版本、CORS、请求聚合以及上游传输错误和领域错误的统一转换。它不持有领域事实；生产环境仅持久化跨服务互动已解析收件人的社区通知工作项，供独立 Worker 投递。

## 对外接口

- `GET /v1/feed`：推荐流。
- `GET /v1/ads`、`POST /v1/ads/events`：显著标识广告的投放决策与幂等曝光/点击回执；事件只能对应同一用户实际收到的短时投放凭证。
- `GET /v1/mall/products*`、`GET/POST /v1/orders*`：商品目录、创建订单、查询订单与取消待支付订单。创建订单必须携带 `Idempotency-Key`。
- `GET/PATCH /v1/me/profile`：当前账户的公开资料读取与更新。
- `GET/PUT /v1/me/creator-profile`、`GET /v1/creators*`：当前创作者经营页的管理，以及按专长、关键词或用户 ID 发现创作者。账户显示名和头像继续由 `account` 提供。
- `POST /v1/messages`、`GET /v1/messages/conversations*`、`POST /v1/messages/conversations/{id}/read`：一对一私信、会话与已读状态；发送必须携带 `Idempotency-Key`。
- `POST /v1/messages/{message_id}/report`：当前接收者举报收到的私信；必须携带 `Idempotency-Key`，响应只返回举报回执而不回传私信正文。
- `GET/PUT /v1/message-preferences`：读取或修改本人是否接收私信。
- `GET /v1/search`、`GET /v1/search/suggestions`：搜索与联想词。用户型搜索结果会附带已启用创作者的行动专长档案，档案服务短暂不可用时保留核心结果并标记 `degraded`。
- `POST /v1/events`：批量上报曝光、点击等用户行为。
- `POST /v1/feedback`、`GET /v1/me/feedback`：提交产品反馈并查看自己的处理状态；提交支持 `Idempotency-Key`。
- `GET /v1/moderation/feedback`、`PATCH /v1/moderation/feedback/{feedback_id}`：受限的反馈队列和人工处理接口。
- `POST /v1/media/upload-url`、`POST /v1/media/{id}/complete`：对象存储直传控制面。
- `GET /v1/media/{id}`：媒体元数据与 CDN 地址。
- `/v1/journeys`、`/v1/today`、`/v1/actions/*`：路线与行动。
- `GET /v1/notifications`、`PATCH /v1/notifications/{notification_id}/read`：用户私有通知收件箱与已读确认。
- `/v1/posts/*`：内容、评论和互动。
- `POST /v1/posts/{id}/knowledge`、`POST /v1/knowledge/{resource_id}/journey`：将当前可见的社区内容收集为私有知识库引用，并在用户确认后原子转换为带首项行动的私人 Journey。
- `GET /v1/users/{user_id}/posts`：按时间读取创作者可见的已公开内容；该接口不会返回草稿、审核中或受限内容。
- `GET /v1/me/posts`、`GET /v1/me/appeals`：当前作者私有的内容状态与申诉历史，支持状态筛选和游标续页。
- `GET /v1/moderation/reports`、`PATCH /v1/moderation/reports/{report_id}`：受限的举报队列与人工处置接口。
- `GET /v1/moderation/message-reports`、`PATCH /v1/moderation/message-reports/{report_id}`：受限的私信举报队列与人工处置接口；仅审核员可读取原始私信上下文，`resolved + restrict_sender` 会阻断后续私信发送。
- `POST /v1/posts/{post_id}/comments/{comment_id}/report`：举报当前用户可见的公开评论，必须携带 `Idempotency-Key`；响应是没有评论正文的回执。
- `POST /v1/comments/{comment_id}/appeals`、`GET /v1/me/comment-appeals`：当前作者提交或查询自己的评论申诉，提交必须携带 `Idempotency-Key`。
- `GET/PATCH /v1/moderation/comment-reports*`、`GET/PATCH /v1/moderation/comment-appeals*`：受限评论举报/申诉队列和人工处置接口；完整评论正文只在审核员响应中返回。
- `POST /v1/posts/{id}/appeals`：受限内容作者提交申诉；`GET/PATCH /v1/moderation/appeals*`：审核员处理申诉与恢复决定。
- `/v1/users/*`：关注、拉黑和静音关系。
- `GET /v1/route-participations`、`POST /v1/routes/{route_id}/join`、`PUT /v1/routes/{route_id}/participation`：恢复、加入和退出公共路线。

`GET /v1/posts/{id}`、`GET /v1/users/{user_id}/posts`、路线读取以及点赞、评论、举报前的内容校验都使用 BBS Link 的公开读取或等价的 `published` 状态过滤；草稿、审核中和受限内容不会经 Gateway 暴露或接收公开互动。详情读取、创作者公开内容、点赞、评论、举报、路线加入和收集到知识库还会执行同一查看者的社交可见性检查，直接内容 ID 或作者 ID 不能绕过拉黑或静音。`POST /v1/posts/{id}/knowledge` 只将标题、摘要、作者、标签与稳定内容引用写入用户私有知识库，绝不复制正文或媒体；源内容下架、受限或关系变化后，客户端必须重新通过公开内容读取确认可见性。相同用户和内容稳定去重，即使原帖编辑也会返回最初收集的私有引用；该操作还会尽力同步普通收藏状态，并记录 `save_knowledge` 高意图推荐信号。`POST /v1/knowledge/{resource_id}/journey` 在 Growth 内部原子创建首项行动并关联资源，资源本身保证重复请求不会重复建计划；该计划的行动真正完成后，Gateway 只会在该 Journey 唯一关联一个受控社区来源时，把 `complete` 归因给原内容，多来源歧义一律不归因。退出已加入路线不依赖内容可见性，避免关系变更后无法退出。评论作者可通过 `DELETE /v1/posts/{post_id}/comments/{comment_id}` 删除自己的历史评论；这条撤回路径由评论服务按可信身份原子校验所有权，不因之后的社交关系变化而失去撤回能力。生产环境中，点赞、评论和关注成功后会以稳定来源键提交 `community_notification_jobs`，再由独立 Worker 重试写入对方的社区收件箱；公开回复还会分别通知帖子作者与父评论作者，后者与前者相同或属于回复者时只保留一条或不投递。点赞、收藏、隐藏、加入路线或收进知识库成功落库后，Gateway 会生成固定 UUID 的推荐事件；若动作来自 Feed 或搜索结果，会一并传递 `request_id`、会话、零基位置和来源，由 User Event 再核验，伪造或过期归因绝不进入训练与评估。隐藏可携带仅适用于 `hide` 的类型化 `negative_feedback_reason`，而点赞和收藏会拒绝该字段。User Event 短暂不可用只记录降级日志，移动端不再另行以随机 ID 上报，重试不会把同一个有效互动放大。`save_knowledge` 是独立信号，不会额外生成普通收藏事件。Growth 短暂不可用、Worker 崩溃或确认前崩溃都不会丢失通知，重复投递由 Growth 的 `(kind, source_id)` 唯一约束折叠。互动事实属于其他服务，因此无法跨库消除“互动已提交、Gateway 在入队前崩溃”这一窗口；入队失败记录告警但不把已成功的用户操作改写为失败。点赞自己内容、评论自己内容和关注自己不会产生通知。内存模式仅用于本地演示，保留直连降级。事件上报由 Gateway 注入可信用户身份后批量转发给 `user-event`。

`GET /v1/search` 与 `GET /v1/search/suggestions` 会从当前可信身份读取受服务令牌保护的 BBS 可见性策略，合并拉黑/静音作者及拉黑当前用户的作者，并将规范化集合传给内部搜索链路；策略不可用时请求失败而不会以未过滤结果降级。客户端提供的同名查询字段会被 Gateway 覆盖。搜索游标绑定查看者和可见性策略，用户改变拉黑或静音关系后应重新搜索；内容派生的联想词遵循同一策略。

`AUTH_REQUIRED=true` 时 Gateway 校验 HS256 Bearer JWT，并只从已验证的 `sub` 与 `roles` 写入内部 `x-user-id`、`x-user-roles`；来路请求携带的同名头会先被清除。`/v1/me/*` 的作者过滤也只能来自该可信身份，不能由 query string 指定。审核接口还要求角色为 `moderator`、`trust_safety` 或 `admin`，且本地关闭鉴权时也一律拒绝。`resolved + restrict_content` 会通过带 `x-service-token` 的内部调用将内容转为 `restricted`；获准申诉的 `resolved + restore_content` 只能恢复原本受限的内容。Gateway 对两个动作都只尝试一次低延迟快路径，失败只记录告警，绝不把已持久化的审核决定改写为失败；报告下架与获准申诉恢复分别由独立调度器携带服务令牌重试，前者确认公开读取不可用，后者确认公开读取可用后再投递终态通知。

路线加入只接受已发布、当前用户可见且 `content_type=route` 的公开路线；展示用的 `route_title` 从不作为授权依据。Gateway 将 BBS Link 的结构化模板适配为 Growth 的首项行动、附加行动和阶段索引，再由 Growth 事务性创建或复用独立的私人 Journey 并记录参与意图；这是一份采用时快照，不会与原路线保持实时联动。模板后来被编辑不会改变已加入用户的私人计划。缺少模板的历史路线保留一项安全的兼容回退。退出先更新最新意图再写 BBS。同步调用失败时由 `bookway-route-participation-reconciler` 自动重试，BBS 会拒绝低于当前版本的延迟命令。

当采用路线中的行动实际完成时，Growth 在内部完成响应中返回来源路线 ID；Gateway 随后以固定事件 ID 向 User Event 写入一次 `complete` 信号，归因到该公共路线。客户端不再把私有 Action ID 当作内容 ID 上报，因此特征系统只接收已提交、可关联的真实执行反馈。User Event 短暂不可用只会记录降级日志，不会撤销已完成的行动。

私信正文、会话、已读状态、举报与发送限制由 `bbs-message` 持有。发送时服务直接以 BBS 的生成 Client 查询双方关系，只要任一方拉黑便拒绝写入；接收方关闭私信或发送者已被限制时也会拒绝写入。消息与私信通知任务在服务本地事务中一起提交，由专用 Worker 以 `direct-message:{message_id}` 幂等地写入 Growth 收件箱；通知只含会话导航 ID，不复制私信正文。私信列表、已读和举报总是在服务内以可信身份校验，只有原接收者可举报该消息，Gateway 不传递可伪造的举报人、审核员或对方身份。普通举报响应是无正文回执；原消息只由需要 `AUTH_REQUIRED=true` 且角色为 `moderator`、`trust_safety` 或 `admin` 的私信审核队列返回。会话以用户对的稳定标识收敛，`Idempotency-Key` 在发送者范围内唯一，弱网重试返回原消息而复用到不同正文或接收者会冲突。

评论举报在 Gateway 先验证帖子仍公开且对当前查看者可见，再由 Comment 服务在同一可见性集合下确认目标评论可举报；客户端无法指定举报人、隐藏作者集合、申诉作者或审核员。普通举报回执只有举报 ID、评论 ID、原因、状态和时间，绝不回传被举报评论正文。审核员队列才会获得完整上下文，且必须启用 `AUTH_REQUIRED=true` 并拥有 `moderator`、`trust_safety` 或 `admin` 角色。接受举报的 `resolved + restrict_comment` 与获准申诉的 `resolved + restore_comment` 由 Comment 的事务同时改变评论可见性，Gateway 以评论 ID 的稳定来源键向作者投递终态通知。

## 依赖

`account`、`growth`、`bbs-feed`、`search-main`、`user-event`、`bbs-link`、`bbs`、`bbs-creator`、`bbs-message`、`comment`、`interaction-status`、`media`、`content-audit`、`feedback`、`ad-main`、`mall`、`mall-order`。拥有审核角色的可信用户还可使用 `GET /v1/moderation/comments` 领取待审评论，并通过 `PATCH /v1/moderation/comments/{comment_id}` 提交 `{"decision":"approve"|"restrict"}`；批准会按原评论作者（而非审核人）补齐既有的帖子/回复通知并由稳定来源键去重。

## 环境变量

`GATEWAY_ADDR`、`STORAGE_MODE`、`DATABASE_URL`、`ACCOUNT_GRPC_URL`、`GROWTH_GRPC_URL`、`BBS_FEED_GRPC_URL`、`BBS_LINK_GRPC_URL`、`SEARCH_MAIN_GRPC_URL`、`USER_EVENT_GRPC_URL`、`BBS_GRPC_URL`、`BBS_CREATOR_GRPC_URL`、`BBS_MESSAGE_GRPC_URL`、`COMMENT_GRPC_URL`、`INTERACTION_STATUS_GRPC_URL`、`MEDIA_GRPC_URL`、`CONTENT_AUDIT_GRPC_URL`、`FEEDBACK_GRPC_URL`、`AD_MAIN_GRPC_URL`、`MALL_GRPC_URL`、`MALL_ORDER_GRPC_URL`、`AUTH_REQUIRED`、`AUTH_JWT_SECRET`、`SERVICE_AUTH_TOKEN`、`HTTP_CONNECT_TIMEOUT_MS`、`HTTP_REQUEST_TIMEOUT_MS`、`REDIS_URL`、`REDIS_CONNECT_TIMEOUT_MS`、`REDIS_COMMAND_TIMEOUT_MS`、`RATE_LIMIT_PER_MINUTE`。`STORAGE_MODE=postgres` 时 Gateway 需要 `DATABASE_URL` 以提交社区通知任务；审核 JWT 的 `roles` 声明是字符串数组；角色由身份系统签发，不能由客户端请求头提供。

## 生产化待办

当前已接入 JWT、服务令牌、请求 ID、Redis 限流和统一调用超时。下一阶段补齐 OIDC/JWKS 与密钥轮换、接口级限流策略、熔断、OpenTelemetry 上下文传播、OpenAPI 契约和分接口容量压测。
