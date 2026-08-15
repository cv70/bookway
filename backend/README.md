# 万卷行 Backend

万卷行后端是独立的 Rust workspace，统一使用 Rust 2024、Axum、Tokio、Tower、Serde 和 Reqwest。

当前版本是**生产基础设施接入版本**：在线服务可以独立编译和启动，关键跨服务流程由专用 Worker 协调。核心事实数据可通过 `STORAGE_MODE=postgres` 切换到 SQLx/PostgreSQL；用户事件使用同事务 Outbox 并由 Kafka Relay 发布；Redis 提供限流和特征缓存；OpenSearch、对象存储/CDN、内容审核、特征与模型排序、JWT/服务令牌、Prometheus 指标和 SLO 已形成可运行边界。默认 `memory` 模式仍保留用于无依赖开发。

## 微服务拓扑

| 服务 | 默认端口 | 所有权 | 主要职责 |
| --- | ---: | --- | --- |
| `gateway` | `8080` | platform | App 唯一入口、聚合和错误转换 |
| `growth` | `8081` | growth | 私人路线、行动留痕、回望、资源知识库与陪伴简报 |
| `bbs` | `8082` | bbs | 关注、拉黑、静音、公共路线参与和社交图谱 |
| `recommend-main` | `8083` | recommendation | 多路召回、补全、过滤、排序和曝光 |
| `bbs-link` | `8084` | bbs-content | 内容事实、版本和发布状态 |
| `bbs-search` | `8085` | bbs-search | 内容、路线、用户、主题检索与联想 |
| `comment` | `8086` | comment | 评论正文、父子关系和审核状态 |
| `commonlikestatus` | `8087` | interaction | 点赞、收藏、计数和批量互动状态 |
| `bbs-feed` | `8088` | bbs-feed | Feed 产品策略、请求规范化和交付 |
| `user-event` | `8089` | data-platform | 曝光、点击等用户行为批量接收 |
| `search-main` | `8090` | search | 查询规范化、搜索编排和降级边界 |
| `media` | `8091` | platform | 对象 key、上传会话、媒体元数据和 CDN |
| `content-audit` | `8092` | trust-safety | 文本审核、风险决策和人工复审入口 |
| `feature-main` | `8093` | recommendation | 实时/离线特征统一读取与缓存 |
| `recommend-recall` | `8095` | recommendation | 候选召回、去重和已看过滤 |
| `recommend-rank` | `8096` | recommendation | 模型排序、版本、实验桶与启发式降级 |
| `route-participation-reconciler` | - | growth | 将版本化路线参与意图最终收敛到 BBS |
| `reminder-dispatcher` | - | growth | 按偏好与静默时段生成去重的提醒 Outbox 命令 |
| `appeal-notification-dispatcher` | - | trust-safety | 可靠恢复获准内容并投递终态结果到作者私有收件箱 |
| `content-report-restriction-dispatcher` | - | trust-safety | 可靠执行接受举报后的内容下架 |

```text
mobile ──HTTPS──> gateway ─┬──> growth
                           ├──> bbs-link
                           ├──> bbs
                           ├──> comment
                           ├──> commonlikestatus
                           ├──> user-event
                           ├──> media ────────────> S3/MinIO + CDN
                           ├──> search-main ───────> bbs-search ──> bbs-link
                           └──> bbs-feed ─────────> recommend-main ─┬──> bbs-link
                                                                  ├──> bbs
                                                                  ├──> commonlikestatus
                                                                  └──> feature-main ──> recommend-rank
```

数据所有权严格分离：`bbs-link` 持有内容事实，`bbs` 持有关系，`comment` 持有评论，`commonlikestatus` 持有互动状态，`user-event` 持有行为接收幂等状态，`recommend-main` 只负责在线推荐决策，`bbs-feed` 负责 Feed 交付，`search-main` 负责搜索产品编排，`bbs-search` 只负责检索和索引访问。

## 目录约定

```text
backend/
├── bookway/
│   ├── gateway/
│   ├── growth/
│   ├── bbs/
│   ├── recommend-main/
│   ├── bbs-link/
│   ├── bbs-search/
│   ├── comment/
│   ├── commonlikestatus/
│   ├── bbs-feed/
│   ├── user-event/
│   ├── search-main/
│   ├── media/
│   ├── content-audit/
│   ├── feature-main/
│   ├── recommend-recall/
│   └── recommend-rank/
│       ├── Cargo.toml
│       ├── README.md
│       ├── service.yaml
│       ├── src/main.rs
│       ├── src/api/          # HTTP 或 gRPC 传输适配
│       ├── src/conf/
│       ├── src/datasource/
│       └── src/domain/       # Domain 持有配置、依赖和业务编排
├── deploy/docker-compose.yml
├── migrations/
├── cmd/
│   ├── db-migrate/
│   ├── outbox-relay/
│   ├── appeal-notification-dispatcher/
│   ├── content-report-restriction-dispatcher/
│   ├── reminder-dispatcher/
│   ├── route-participation-reconciler/
│   └── search-indexer/
├── infra/data/
├── infra/event/
├── infra/runtime/
├── pkg/api/                  # HTTP DTOs and shared application types
├── Cargo.toml
├── Cargo.lock
└── rust-toolchain.toml
```

每一个微服务的 `README.md` 与该服务的 `Cargo.toml` 同级。服务内部统一由 `Domain` 持有配置、Repository、客户端和业务对象；`api/http.rs` 只承载外部 HTTP，`api/grpc.rs` 只承载内部 gRPC，服务启动入口由 `api` 暴露给 `main`：

```text
main -> Domain -> api/http.rs or api/grpc.rs
          |
     datasource / conf
```

## 推荐主链路

`recommend-main/src/domain/pipeline` 对应 x-algorithm 的多阶段 Candidate Pipeline：

```text
Query Hydrator
-> 质量/新鲜度内容召回（并行）
-> BBS 关系补全
-> commonlikestatus 互动状态补全
-> 去重、已看、安全过滤
-> 质量、意图、作者多样性打分
-> 多样性 Selector
-> 异步曝光副作用
```

候选源失败或补全失败时保留可用结果，并将 `FeedMeta.degraded` 置为 `true`。响应包含 `request_id`、`pipeline_id`、游标、阶段统计、真实 `author_id`、模型版本和实验桶。在线链路已调用 `feature-main` 和 `recommend-rank`；用户 `hide` 负反馈会持久化并在下一次推荐中硬过滤，多样性 Selector 对作者、领域和标签使用局部窗口约束，并在窄候选池中逐级放宽以避免饿死。`surface=following` 在社交上下文补全后只保留已关注作者，且已看历史按 surface 隔离。当前模型实现是可解释、可版本化的启发式 Ranker，远程排序不可用时保留流水线基础分并显式标记降级。训练平台、外部推理引擎和可回放评测仍属于后续容量阶段。

## Feed 与推荐链路

Gateway 只请求 `bbs-feed`。`bbs-feed` 负责客户端 surface、分页上限和产品降级边界，再调用 `recommend-main` 完成候选流水线。关注流是显式的 `following` surface，由 `bbs` 社交图补全后过滤候选；Gateway 另提供 `/v1/social/context` 恢复客户端关系状态。路线参与通过 `/v1/route-participations` 恢复，并由 `/v1/routes/{route_id}/participation` 幂等写入；客户端加入使用 `/v1/routes/{route_id}/join`。Growth 在创建 Journey 的同一事务中写入版本化参与意图，Gateway 同步调用 BBS 作为低延迟快路径，`route-participation-reconciler` 使用数据库租约、并发批处理和无限退避补偿失败写入。BBS 持久化最后应用版本并原子拒绝旧命令，因此加入、退出和重新加入在超时、乱序及 Worker 崩溃后仍以最新用户意图为准。热门路线写入只串行同一用户的同一路线命令，活跃人数由 64 个事务计数分片维护；推荐与搜索以批量 BBS 上下文聚合分片，不逐候选发 RPC，也不扫描热门路线的完整参与事实。

## 成长陪伴链路

Gateway 的 `GET /v1/companion?date=YYYY-MM-DD&timezone=Asia/Shanghai` 代理 Growth 的只读陪伴简报；`GET /v1/today` 接受相同的本地日期上下文。它只查看用户自己的进行中路线、今日行动和回望快照，按状态返回 `start_small`、`keep_going`、`celebrate` 或 `plan_next`，并提供选择该行动的原因与反思问题。建议需要用户主动打开行动后才会进入既有操作流程；请求本身不会完成、跳过、改期或修改预计时长。

行动契约包含展示用 `scheduled_label`、带显式 UTC 偏移量的 RFC 3339 `scheduled_for` 和 IANA `scheduled_timezone`。PostgreSQL 使用本地日期索引今日清单，并独立存储精确瞬间与时区；陪伴策略因此可为明确安排但已过期的行动建议恢复入口。旧行动不会被补造成伪精确时间，也不会被误判为逾期；提醒窗口、静默时段、去重投递与用户通知收件箱已实现，自动改期和实际推送 provider 消费仍需后续实现。

## 搜索链路

Gateway 只请求 `search-main`，由它规范化参数和编排底层 `bbs-search`：

```text
内部搜索由 `search-main` 的 gRPC `search` 和 `suggestions` 方法提供。
```

设置 `OPENSEARCH_URL` 后，`bbs-search` 使用 OpenSearch 多字段召回并在索引不可用时降级到 `bbs-link`；`bookway-search-indexer` 创建版本化 CJK 索引并同步公开内容。`search-main` 仍承担 `query rewrite -> recall -> pre-rank -> rank -> rerank` 的产品编排边界。

搜索游标采用 `v2 + 查询/类型指纹 + 短期会话 ID`，不同查询或结果类型之间不可复用，且客户端游标大小受限。会话在服务端保存未消费的混合结果、去重键和源游标；OpenSearch 主路径以 5 分钟 PIT 和 `_score` / 内容 ID 的 `search_after` 续页，不再截断为固定候选集。PIT 或会话过期时返回可识别的前置条件错误，客户端应从第一页重新搜索；索引首次请求不可用时仍会降级到 `bbs-link`。联想词按近 90 天真实查询统计、命中内容和冷启动词合并去重。

## 内容、社区与互动

- `bbs-link`：草稿、审核中、已发布、受限、删除状态；作者归属、版本号和 `Idempotency-Key` 请求指纹。
- `bbs`：关注、拉黑、静音和公共路线参与；拉黑清理关注并阻止冲突关注，路线参与支持退出、重入、私人 Journey 关联和按路线批量计数。
- `comment`：空评论/超长评论/跨帖父评论校验，支持回复、写入幂等和 `(created_at, id)` 稳定游标分页；新评论先待审，只有 `content-audit` 通过后才会进入公开列表，审核故障 fail-closed。
- `commonlikestatus`：点赞、收藏与 `hide` 负反馈的幂等集合、计数和批量互动上下文；隐藏内容供推荐在线硬过滤。
- Gateway：互动写入前调用内容服务校验内容存在且公开；审核员 JWT 角色受限地开放举报队列和处置入口。
- `content-audit`：接收按用户和幂等键去重的内容举报与作者申诉；内部人工队列支持稳定游标、认领、结案说明、不可覆盖的终态决定，以及持久化的 `restrict_content` / `restore_content` 动作；终态决定与对应的下架、恢复或通知任务同事务提交。
- `growth`：持有私人资源知识库、阅读进度、书签、路线关联、检索条件、只读陪伴简报和提醒偏好/设备注册；创建使用用户级 `Idempotency-Key`，所有关联路线均校验归属。
- `user-event`：最多 100 条批量事件、UUID 与时间校验、`event_id` 幂等；事件和 Outbox 同事务写入，`bookway-outbox-relay` 负责 Kafka 重试、指数退避与死信。

配置 `CONTENT_AUDIT_GRPC_URL` 后，发布会先进入 `content-audit`，再转换为公开、复审或受限状态；每个内容版本保留风险分数、原因和 provider。举报和申诉的人工决定在 PostgreSQL 中保存审核员、说明与动作；通过已验证 JWT 角色的 `resolved + restrict_content` / `resolved + restore_content` 决定会使用内部服务令牌幂等地改变公开状态。终态举报与 `content_report_restriction_jobs` 在同一审核事务中提交，`bookway-content-report-restriction-dispatcher` 以租约、退避与服务令牌可靠重放下架，直到 `bbs-link.get_public` 不再可读。终态申诉与 `content_appeal_notification_jobs` 也在一个审核事务中提交，`bookway-appeal-notification-dispatcher` 可靠重放获准恢复，再以幂等来源键投递 Growth 收件箱；恢复公开只会在 `bbs-link.get_public` 已确认公开后通知作者。申诉 SLA、双人复核和多媒体审核仍需继续扩展。

媒体上传通过 Gateway 获取 S3/MinIO 预签名 PUT URL，客户端直接上传到对象存储，再调用完成接口。`media` 会使用 HEAD 校验对象的实际大小和 MIME，只有校验一致才将元数据从 `pending` 转为 `ready`；待上传资产仅所有者可读，`ready` 资产通过 CDN URL 访问。

## 数据库与本地依赖

迁移按服务所有权拆分：

| 文件 | 服务 | 内容 |
| --- | --- | --- |
| `0001_content.sql` | bbs-link | 内容、媒体、主题、幂等键 |
| `0002_bbs.sql` | bbs | 社交关系 |
| `0003_search.sql` | bbs-search | 搜索文档和查询统计 |
| `0004_feed.sql` | recommend-main | 曝光和推荐事件 |
| `0005_commonlikestatus.sql` | commonlikestatus | 点赞/收藏状态 |
| `0006_comment.sql` | comment | 评论和审核状态 |
| `0007_user_event.sql` | user-event | 用户行为事件、时间和请求关联索引 |
| `0008_growth.sql` | growth | 私人路线与今日行动 |
| `0009_outbox.sql` | data-platform | Transactional Outbox 与审计日志 |
| `0010_media_audit_features.sql` | platform | 媒体、审核、特征和模型版本 |
| `0011_feedback_reviews_search.sql` | recommendation / growth / search / trust-safety | 隐藏反馈、行动留痕、搜索热词索引和社区举报队列 |
| `0012_knowledge_companionship.sql` | growth | 私人资源知识库、阅读状态、书签、检索索引和幂等写入 |
| `0013_route_participation.sql` | bbs | 公共路线参与、私人路线关联和活跃同行人数索引 |
| `0014_idempotent_route_join.sql` | growth | 公共路线来源唯一绑定和跨服务加入重试去重 |
| `0015_route_participation_reconciliation.sql` | growth / bbs | 路线参与期望状态、协调租约与乱序写保护 |
| `0016_route_participation_sharded_counts.sql` | bbs | 热门路线 64 分片活跃计数、事务触发器和存量回填 |
| `0017_user_event_personalization.sql` | user-event / recommendation | 高意图与安全事件类型、候选级个性化查询索引 |
| `0018_comment_scale.sql` | comment | 评论写入幂等键与稳定游标分页索引 |
| `0019_action_scheduling.sql` | growth | 行动精确安排瞬间、IANA 时区与本地日期索引 |
| `0020_reminder_delivery.sql` | growth | 提醒偏好、设备端点、静默窗口与去重投递记录 |
| `0021_notification_inbox.sql` | growth | 用户通知收件箱、已读状态与生产端幂等键 |
| `0022_search_sessions.sql` | bbs-search | 短期搜索会话和稳定游标状态 |
| `0023_comment_moderation.sql` | comment | 公开评论审核读取索引 |
| `0024_content_appeals.sql` | trust-safety | 内容作者申诉、审核队列与幂等键 |
| `0025_content_appeal_owner_lookup.sql` | trust-safety | 作者私有申诉历史的游标查询索引 |
| `0026_content_appeal_notification_jobs.sql` | trust-safety | 终态申诉的可靠收件箱投递任务 |
| `0027_content_report_restriction_jobs.sql` | trust-safety | 接受举报后的可靠内容下架任务 |

```bash
docker compose -f deploy/docker-compose.yml up -d
```

该命令启动 PostgreSQL 16、Redis 7、Redpanda、OpenSearch 2.17、MinIO 和 Prometheus，并在 Redpanda 健康后幂等创建 12 分区的 `bookway.domain-events.v1` Topic。复制 `.env.example` 中需要的变量并显式设置 `STORAGE_MODE=postgres`；业务服务不会自动执行迁移。

```bash
DATABASE_URL=postgres://bookway:bookway-local-only@127.0.0.1:5432/bookway \
cargo run -p bookway-db-migrate
cargo run -p bookway-outbox-relay
cargo run -p bookway-reminder-dispatcher
cargo run -p bookway-route-participation-reconciler
cargo run -p bookway-appeal-notification-dispatcher
cargo run -p bookway-content-report-restriction-dispatcher
cargo run -p bookway-search-indexer
```

## 本地运行

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

依次启动：

```bash
cargo run -p bookway-bbs
cargo run -p bookway-bbs-link
cargo run -p bookway-bbs-search
cargo run -p bookway-comment
cargo run -p bookway-commonlikestatus
cargo run -p bookway-recommend-main
cargo run -p bookway-bbs-feed
cargo run -p bookway-user-event
cargo run -p bookway-search-main
cargo run -p bookway-media
cargo run -p bookway-content-audit
cargo run -p bookway-feature-main
cargo run -p bookway-recommend-recall
cargo run -p bookway-recommend-rank
cargo run -p bookway-growth
cargo run -p bookway-gateway
```

主要环境变量：

| 服务 | 监听变量 | 上游变量 |
| --- | --- | --- |
| gateway | `GATEWAY_ADDR` | `GROWTH_GRPC_URL`、`BBS_FEED_GRPC_URL`、`SEARCH_MAIN_GRPC_URL`、`USER_EVENT_GRPC_URL`、`BBS_LINK_GRPC_URL`、`BBS_GRPC_URL`、`COMMENT_GRPC_URL`、`LIKE_STATUS_GRPC_URL`、`MEDIA_GRPC_URL`、`CONTENT_AUDIT_GRPC_URL` |
| growth | `GROWTH_ADDR` | 无 |
| bbs | `BBS_ADDR` | 无 |
| recommend-main | `RECOMMEND_MAIN_ADDR` | `BBS_GRPC_URL`、`LIKE_STATUS_GRPC_URL`、`FEATURE_MAIN_GRPC_URL`、`RECOMMEND_RECALL_GRPC_URL`、`RECOMMEND_RANK_GRPC_URL` |
| bbs-link | `BBS_LINK_ADDR`、`BBS_LINK_GRPC_ADDR` | `CONTENT_AUDIT_GRPC_URL` |
| comment | `COMMENT_ADDR`、`COMMENT_GRPC_ADDR` | `CONTENT_AUDIT_GRPC_URL`（持久化模式缺失时评论 fail-closed） |
| bbs-search | `BBS_SEARCH_ADDR` | `BBS_LINK_GRPC_URL` |
| commonlikestatus | `LIKE_STATUS_ADDR` | 无（由 Gateway 先校验内容） |
| bbs-feed | `BBS_FEED_ADDR` | `RECOMMEND_MAIN_GRPC_URL` |
| user-event | `USER_EVENT_ADDR` | PostgreSQL + Transactional Outbox，由 Relay 发布 Kafka |
| route-participation-reconciler | 无监听端口 | `DATABASE_URL`、`BBS_GRPC_URL`、`ROUTE_RECONCILE_*` |
| reminder-dispatcher | 无监听端口 | `DATABASE_URL`、`REMINDER_DISPATCH_BATCH_SIZE`、Outbox Relay |
| appeal-notification-dispatcher | 无监听端口 | `DATABASE_URL`、`BBS_LINK_GRPC_URL`、`GROWTH_GRPC_URL`、`APPEAL_NOTIFICATION_*` |
| content-report-restriction-dispatcher | 无监听端口 | `DATABASE_URL`、`BBS_LINK_GRPC_URL`、`REPORT_RESTRICTION_*` |
| search-main | `SEARCH_MAIN_ADDR` | `BBS_SEARCH_GRPC_URL` |
| media | `MEDIA_ADDR` | `S3_ENDPOINT`、`S3_BUCKET`、`CDN_BASE_URL` |
| content-audit | `CONTENT_AUDIT_ADDR` | 审核规则与 PostgreSQL |
| feature-main | `FEATURE_MAIN_ADDR` | `REDIS_URL`、PostgreSQL |
| recommend-recall | `RECOMMEND_RECALL_ADDR` | `BBS_LINK_GRPC_URL` |
| recommend-rank | `RECOMMEND_RANK_ADDR` | `RECOMMEND_RANK_MODEL_VERSION` |

全服务共享 `STORAGE_MODE`、`DATABASE_URL`、`SERVICE_AUTH_TOKEN`、`SERVICE_AUTH_REQUIRED`、`AUTH_REQUIRED`、`AUTH_JWT_SECRET`、`HTTP_CONNECT_TIMEOUT_MS` 和 `HTTP_REQUEST_TIMEOUT_MS`。Gateway 的 `CORS_ALLOWED_ORIGINS` 必须列出允许访问 Web API 的精确 `http(s)` Origin，拒绝通配符、路径和自定义 scheme；默认值仅供本机 Expo Web 调试。Redis 连接和命令预算分别由 `REDIS_CONNECT_TIMEOUT_MS`（默认 1000ms）与 `REDIS_COMMAND_TIMEOUT_MS`（默认 100ms）控制，缓存或限流 Redis 故障时会告警并 fail-open。生产环境必须启用两种鉴权：App 的 Bearer JWT 只在 Gateway 解析；非 Gateway 服务的业务端点只接受带服务令牌的内部请求，并信任 Gateway 注入的 `x-user-id`。健康、就绪和指标端点不要求服务令牌。

如果默认端口被占用，可整组使用 `18080-18090` 并显式设置全部 URL；客户端将 `EXPO_PUBLIC_API_URL` 指向新的 Gateway 地址。

## 已实现的生产能力

- SQLx 连接池和按服务所有权拆分的 PostgreSQL Repository，写路径使用事务与幂等键。
- Redis 全局限流和特征缓存；Redis 故障 fail-open 并记录告警。
- Transactional Outbox、Kafka/Redpanda Relay、`SKIP LOCKED`、退避和死信状态。
- OpenSearch CJK 索引、版本化索引名、检索降级和独立重建命令。
- Gateway 媒体 API、MinIO/S3 预签名直传、对象大小/MIME 完成校验、私有 pending 元数据和 CDN 地址。
- `feature-main -> recommend-rank` 在线调用，推荐模型失败时保留启发式得分并标记降级。
- `feature-main` 从近 90 天行为构造领域/作者亲和度、重复曝光疲劳和直接负反馈候选特征；`recommend-rank` 在用户级特征之外执行候选级个性化。
- `hide` 负反馈闭环、局部窗口多样性重排、模型/实验诊断元数据和来源降级传播。
- 动态搜索联想、查询统计、查询绑定游标，以及行动留痕与服务端周回望聚合。
- 基于进行中路线、真实行动状态和精确安排时间的只读陪伴简报；建议由用户显式采纳，服务不会隐式改写计划。
- 用户举报与作者申诉的幂等接入、PostgreSQL 人工队列、稳定续页、审核认领/结案、角色受限的下架与恢复动作；举报下架、获准内容恢复与终态通知均通过事务任务、租约调度与幂等写入可靠收敛。
- 评论回复、弱网写入幂等、稳定游标分页，以及移动端失败草稿恢复和加载更多。
- 私人资源知识库、阅读进度与书签持久化，以及基于真实作者 ID 的关注关系和独立关注流。
- 公共路线参与持久化、跨端加入状态恢复、真实同行人数，以及推荐/搜索的批量上下文补全。
- 服务端幂等路线加入编排；并发、超时或客户端重启后的重试会复用同一私人 Journey。
- 路线参与 Saga 使用版本化期望状态、`SKIP LOCKED` 租约协调器和 BBS 原子版本门禁；同步双写失败、乱序完成及进程崩溃均可自动收敛。
- 热门路线参与使用用户级并发控制和 64 分片事务计数，重复/乱序命令不重复计数，批量上下文不再扫描完整参与事实。
- Gateway HS256 JWT、下游服务 token、全服务请求 ID、调用超时、Prometheus 指标、按服务依赖 readiness 与 SLO 文档。

## 下一阶段阻断项

1. PostgreSQL 分片、读写隔离、跨地域复制、自动故障切换和备份恢复演练。
2. Kafka 幂等消费者、Schema 管理、事件回放工具与离线数仓/湖仓落地。
3. 图片处理、视频转码、病毒扫描、媒体审核和 CDN 回源保护。
4. OpenTelemetry OTLP trace、日志关联、持续压测和完整容量模型。
5. 具备 MFA、双人复核与完整审计查询的运营审核台、申诉 SLA、死信任务人工补偿、反作弊、未成年人和区域合规策略。
6. 模型训练、特征管理、A/B 平台、影子流量、漂移监控和一键回滚。
7. 提醒窗口、静默时段、通知投递和用户确认的自动改期策略。
8. 多区域容灾、Kubernetes/Service Mesh、CI/CD、密钥托管和供应链安全。

应按压测、延迟、成本和故障域推进后续拆分，而不是仅以服务数量判断是否生产化。
