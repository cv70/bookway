# 万卷行 Backend

万卷行后端是独立的 Rust workspace，统一使用 Rust 2024、Axum、Tokio、Tower、Serde 和 Reqwest。

当前版本是**生产基础设施接入版本**：在线服务可以独立编译和启动，关键跨服务流程由专用 Worker 协调。核心事实数据可通过 `STORAGE_MODE=postgres` 切换到 SQLx/PostgreSQL；用户事件使用同事务 Outbox 并由 Kafka Relay 发布；Redis 提供限流和特征缓存；OpenSearch、对象存储/CDN、内容审核、特征与模型排序、JWT/服务令牌、Prometheus 指标和 SLO 已形成可运行边界。`STORAGE_MODE` 必须显式设置（`postgres` 或 `memory`）：未设置时服务拒绝启动——内存模式带演示种子数据，生产部署漏配环境变量时静默启用等于端出假数据。`memory` 仅供无依赖本地开发。

## 微服务拓扑

| 服务 | 默认端口 | 所有权 | 主要职责 |
| --- | ---: | --- | --- |
| `gateway` | `8080` | platform | App 唯一入口、聚合和错误转换 |
| `account` | `8094` | account | 用户公开资料和资料更新 |
| `growth` | `8081` | growth | 私人路线、行动留痕、回望、资源知识库与陪伴简报 |
| `bbs` | `8082` | bbs | 关注、拉黑、静音、公共路线参与和社交图谱 |
| `bbs-creator` | `8105` | community-creator | 创作者定位、专长、精选内容和公开经营档案 |
| `bbs-message` | `8106` | community-message | 一对一私信、会话、已读、举报、发送限制与通知 Outbox |
| `recommend-main` | `8083` | recommendation | 多路召回、补全、过滤、排序和曝光 |
| `bbs-link` | `8084` | bbs-content | 内容事实、版本和发布状态 |
| `bbs-search` | `8085` | bbs-search | 内容、路线、用户、主题检索与联想 |
| `comment` | `8086` | comment | 评论正文、父子关系、审核、举报与申诉状态 |
| `interaction-status` | `8087` | interaction | 点赞、收藏、计数和批量互动状态 |
| `bbs-feed` | `8088` | bbs-feed | Feed 产品策略、请求规范化和交付 |
| `user-event` | `8089` | data-platform | 曝光、点击等用户行为批量接收 |
| `search-main` | `8090` | search | 查询规范化、搜索编排和降级边界 |
| `media` | `8091` | platform | 对象 key、上传会话、媒体元数据和 CDN |
| `content-audit` | `8092` | trust-safety | 文本审核、风险决策和人工复审入口 |
| `feedback` | `8104` | product-experience | 用户产品反馈、状态跟踪和受限人工处理队列 |
| `feature-main` | `8093` | recommendation | 实时/离线特征统一读取与缓存 |
| `recommend-recall` | `8095` | recommendation | 候选召回、去重和已看过滤 |
| `recommend-rank` | `8096` | recommendation | 模型排序、版本、实验桶与启发式降级 |
| `ad-center` | `8097` | commercial-advertising | 广告活动、预算、投放凭证与事件账本 |
| `ad-recall` | `8098` | commercial-advertising | 活动召回、定向与频次约束 |
| `ad-rank` | `8099` | commercial-advertising | 广告候选的版本化排序 |
| `ad-main` | `8100` | commercial-advertising | 广告决策编排与事件入口 |
| `mall` | `8101` | commercial-commerce | 商品、SKU、售价与售卖状态 |
| `mall-inventory` | `8102` | commercial-commerce | 可用库存与可恢复的订单预占 |
| `mall-order` | `8103` | commercial-commerce | 订单快照、支付状态与库存 Saga |
| `route-participation-reconciler` | - | growth | 将版本化路线参与意图最终收敛到 BBS |
| `reminder-dispatcher` | - | growth | 按偏好与静默时段生成去重的提醒 Outbox 命令 |
| `push-delivery-dispatcher` | - | growth | 租约化发送行动提醒、退避重试并撤销失效设备 |
| `community-notification-dispatcher` | - | gateway | 租约化投递点赞、评论和关注的社区收件箱通知 |
| `direct-message-notification-dispatcher` | - | community-message | 租约化投递无正文私信导航通知到收件箱 |
| `appeal-notification-dispatcher` | - | trust-safety | 可靠恢复获准内容并投递终态结果到作者私有收件箱 |
| `content-report-restriction-dispatcher` | - | trust-safety | 可靠执行接受举报后的内容下架 |
| `mall-order-expirer` | - | commercial-commerce | 批量过期未支付订单并补偿释放预占库存 |
| `mall-inventory-sweeper` | - | commercial-commerce | 独立回收超时库存预占 |

```text
mobile ──HTTPS──> gateway ─┬──> growth
                           ├──> account
                           ├──> bbs-link
                           ├──> bbs
                           ├──> bbs-creator
                           ├──> bbs-message ──────> bbs
                           │         └────────────> Growth inbox
                           ├──> comment
                           ├──> interaction-status
                           ├──> user-event
                           ├──> media ────────────> S3/MinIO + CDN
                           ├──> ad-main ──────────> ad-recall ──> ad-center
                           │                            └──────> ad-rank
                           ├──> mall
                           └──> mall-order ───────> mall-inventory
                           ├──> feedback
                           ├──> search-main ───────> bbs-search ──> bbs-link
                           └──> bbs-feed ─────────> recommend-main ─┬──> bbs-link
                                                                  ├──> bbs
                                                                  ├──> interaction-status
                                                                  └──> feature-main ──> recommend-rank
```

数据所有权严格分离：`account` 持有公开资料，`bbs-link` 持有内容事实，`bbs` 持有关系，`bbs-creator` 持有创作者经营档案，`bbs-message` 持有会话、消息、私信意愿、举报、发送限制与本地通知 Outbox，`comment` 持有评论、评论举报和评论申诉，`interaction-status` 持有互动状态，`user-event` 持有行为接收幂等状态，`recommend-main` 只负责在线推荐决策，`bbs-feed` 负责 Feed 交付，`search-main` 负责搜索产品编排，`bbs-search` 只负责检索和索引访问，`feedback` 持有用户提交的产品反馈与处理状态。广告活动、预算和可验证投放凭证由 `ad-center` 持有，`ad-main` 只编排投放；商品目录、库存和订单分别由 `mall`、`mall-inventory`、`mall-order` 持有。Gateway 只持有跨服务互动中已解析接收者的社区通知投递工作项，不复制点赞、评论或关注事实。`bbs-message` 在写入前直接调用 BBS 生成 Client 检查双方的 block 边，不复制关系数据。`mall-order-expirer` 和 `mall-inventory-sweeper` 只通过受服务令牌保护的内部 gRPC 执行过期补偿，不持有业务事实。JWT 与其他认证凭证不属于 `account` 的数据所有权。

## 目录约定

```text
backend/
├── bookway/
│   ├── gateway/
│   ├── account/
│   ├── growth/
│   ├── bbs/
│   ├── bbs-creator/
│   ├── bbs-message/
│   ├── recommend-main/
│   ├── bbs-link/
│   ├── bbs-search/
│   ├── comment/
│   ├── interaction-status/
│   ├── bbs-feed/
│   ├── user-event/
│   ├── search-main/
│   ├── media/
│   ├── content-audit/
│   ├── feedback/
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
├── deploy/systemd/           # 长驻服务与维护作业的 systemd 单元（索引器/对账/评估）
├── bookway-py/               # Python 侧作业与服务（job/cronjob/bg 三类）
│   ├── cronjob/rank_training/  # 排序模型训练（PyTorch：logistic artifact + MiniCPM LoRA）
│   └── bg/model_serving/       # MiniCPM5-1B 推理常驻服务（embeddings + LLM 打分）
├── migrations/
├── cmd/
│   ├── db-migrate/
│   ├── outbox-relay/
│   ├── appeal-notification-dispatcher/
│   ├── content-report-restriction-dispatcher/
│   ├── mall-order-expirer/
│   ├── mall-inventory-sweeper/
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

每一个微服务的 `README.md` 与该服务的 `Cargo.toml` 同级。服务内部统一由 `Domain` 持有配置、Dao、客户端和业务对象；`api/http.rs` 只承载外部 HTTP，`api/grpc.rs` 只承载内部 gRPC，服务启动入口由 `api` 暴露给 `main`：

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
-> interaction-status 互动状态补全
-> 去重、客户端已看、安全过滤
-> 质量、意图、作者多样性打分
-> 多样性 Selector（优先未曝光、受控回补）
-> 持久化曝光
```

候选源失败或补全失败时保留可用结果，并将 `FeedMeta.degraded` 置为 `true`。响应包含 `request_id`、`pipeline_id`、游标、阶段统计、真实 `author_id`、模型版本和实验桶。曝光在返回 Feed 前持久化，因此 `user-event` 能按可信用户、会话、请求、内容和零基位置批量核验归因；校验不可用时保留用户反馈但清除归因字段，伪造归因被拒绝。在线链路已调用 `feature-main` 和 `recommend-rank`；用户 `hide` 负反馈会持久化并在下一次推荐中硬过滤，多样性 Selector 对作者、领域和标签使用局部窗口约束，并在窄候选池中逐级放宽以避免饿死。服务端近期曝光只作为“优先未曝光”的软约束，候选不足时才受控回补并在原因中说明，避免 Feed 中断；客户端 `seen` 仍按 surface 隔离且硬过滤。`surface=following` 从社交上下文约束召回为已关注作者，并让 cursor 绑定规范化关注集合；关系变动时从当前集合的第一页重启，绝不复用旧集合的 offset。当前模型实现是可解释、可版本化的启发式 Ranker，远程排序不可用时保留流水线基础分并显式标记降级。`bookway-recommendation-evaluator` 能对固定时间窗内的可信曝光归因生成可回放的观察性评测快照；训练平台、外部推理引擎、反事实实验和自动模型发布仍属于后续容量阶段。

## Feed 与推荐链路

Gateway 只请求 `bbs-feed`。`bbs-feed` 负责客户端 surface、分页上限和产品降级边界，再调用 `recommend-main` 完成候选流水线。关注流是显式的 `following` surface，由 `bbs` 社交图约束候选召回并由 BBS Link 按最新顺序返回；Gateway 另提供 `/v1/social/context` 恢复客户端关系状态。路线参与通过 `/v1/route-participations` 恢复，并由 `/v1/routes/{route_id}/participation` 幂等写入；客户端加入使用 `/v1/routes/{route_id}/join`。Growth 在创建 Journey 的同一事务中写入版本化参与意图，Gateway 同步调用 BBS 作为低延迟快路径，`route-participation-reconciler` 使用数据库租约、并发批处理和无限退避补偿失败写入。BBS 持久化最后应用版本并原子拒绝旧命令，因此加入、退出和重新加入在超时、乱序及 Worker 崩溃后仍以最新用户意图为准。热门路线写入只串行同一用户的同一路线命令，活跃人数由 64 个事务计数分片维护；推荐与搜索以批量 BBS 上下文聚合分片，不逐候选发 RPC，也不扫描热门路线的完整参与事实。

## 成长陪伴链路

Gateway 的 `GET /v1/companion?date=YYYY-MM-DD&timezone=Asia/Shanghai` 代理 Growth 的只读陪伴简报；`GET /v1/today` 接受相同的本地日期上下文。它只查看用户自己的进行中路线、今日行动和回望快照，按状态返回 `start_small`、`keep_going`、`celebrate` 或 `plan_next`，并提供选择该行动的原因与反思问题。建议需要用户主动打开行动后才会进入既有操作流程；请求本身不会完成、跳过、改期或修改预计时长。

行动契约包含展示用 `scheduled_label`、带显式 UTC 偏移量的 RFC 3339 `scheduled_for` 和 IANA `scheduled_timezone`。PostgreSQL 使用本地日期索引今日清单，并独立存储精确瞬间与时区；陪伴策略因此可为明确安排但已过期的行动建议恢复入口。旧行动不会被补造成伪精确时间，也不会被误判为逾期；提醒窗口、静默时段、去重投递、用户通知收件箱与 Provider 投递 Worker 已形成闭环。用户改期、完成、跳过、禁用提醒或注销设备都会取消未发送投递；Worker 发送前再次验证行动版本与设备状态，Provider 以稳定 delivery ID 去重。

## 搜索链路

Gateway 只请求 `search-main`，由它规范化参数和编排底层 `bbs-search`：

```text
内部搜索由 `search-main` 的 gRPC `search` 和 `suggestions` 方法提供。
```

设置 `OPENSEARCH_URL` 后，`bbs-search` 通过 `OPENSEARCH_READ_ALIAS` 使用 OpenSearch 多字段召回并在索引不可用时降级到 `bbs-link`；`bookway-search-indexer` 以内容版本写入 `OPENSEARCH_WRITE_INDEX` 指定的物理 CJK 索引，拒绝把别名当成写目标，并能在重建期间双写 `OPENSEARCH_SHADOW_WRITE_INDEX`。`bookway-search-index-rebuild` 使用可恢复的 keyset Bulk 重建补齐历史内容，`bookway-search-index-reconcile` 只读比对物理索引的逐内容可见性/版本和总数，完整扫描的 `healthy=true` 才可作为发布前完整性证据；`bookway-search-index-alias-switch` 随后原子发布读别名，保留旧索引以便回滚；`bookway-search-index-outbox-recovery` 默认输出并审计死信状态，只有具名、注明原因的恢复运行才会重排死信。已有 PIT 继续使用旧快照，新 PIT 使用新别名。语义召回：`bookway-search-indexer` 配置 `SEMANTIC_VECTOR_DIMS`（并指向 `KNOWLEDGE_CATALOG_GRPC_URL`）后，会把标题/摘要/节点/装备的 embedding 写入 `semantic_vector`（knn_vector, cosinesimil, HNSW/lucene），维度一旦写入不可更改；`bbs-search` 新增 `SearchSemantic`（kNN 一次批量、无游标，查询直接遍历读别名索引的 HNSW 图，发布状态/作者可见性与实体面过滤在 k-NN 子句内下推），`search-main` 在内容型与节点/装备查询上以独立"semantic"召回路接入（60ms 预算×2，任一失败该路静默为空），索引无向量时行为与原先完全一致。`search-main` 承担 `query rewrite -> recall -> pre-rank -> rank -> rerank` 的产品编排边界：改写词典使用原子活动版本指针热切换，身份/话题查询不扩展，每个曝光保留改写版本而不保存查询明文；`bookway-search-evaluator` 以可信归因回放版本级观察性质量，不能自动推广词典。

搜索游标采用 `v2 + 查询/类型指纹 + 短期会话 ID`，不同查询或结果类型之间不可复用，且客户端游标大小受限。会话在服务端保存未消费的混合结果、去重键和源游标；OpenSearch 主路径以 5 分钟 PIT 和 `_score` / 内容 ID 的 `search_after` 续页，不再截断为固定候选集。PIT 或会话过期时返回可识别的前置条件错误，客户端应从第一页重新搜索；索引首次请求不可用时仍会降级到 `bbs-link`。联想词按近 90 天真实查询统计、命中内容和冷启动词合并去重。

## 内容、社区与互动

- `bbs-link`：草稿、审核中、已发布、受限、删除状态；作者归属、版本号和 `Idempotency-Key` 请求指纹。
- `bbs`：关注、拉黑、静音和公共路线参与；拉黑清理关注并阻止冲突关注，路线参与支持退出、重入、私人 Journey 关联和按路线批量计数。
- `comment`：空评论/超长评论/跨帖父评论校验，支持回复、写入幂等和 `(created_at, id)` 稳定游标分页；新评论先待审，只有 `content-audit` 通过后才会进入公开列表，审核故障 fail-closed。举报仅针对当前可见的公开评论，作者可对受限评论申诉；`restrict_comment` 与 `restore_comment` 在审核事务内直接改变评论状态。
- `interaction-status`：点赞、收藏与 `hide` 负反馈的幂等集合、计数和批量互动上下文；隐藏内容供推荐在线硬过滤。
- Gateway：互动写入前调用内容服务校验内容存在且公开；审核员 JWT 角色受限地开放举报队列和处置入口。
- 路线打卡复盘：`milestone` 写入前由 Gateway 以 BBS 的活动参与事实做 fail-closed 校验，再由 BBS Link 固化公开路线/阶段快照，防止客户端伪造 WEGU 完成证据。
- `content-audit`：接收按用户和幂等键去重的内容举报与作者申诉；内部人工队列支持稳定游标、认领、结案说明、不可覆盖的终态决定，以及持久化的 `restrict_content` / `restore_content` 动作；终态决定与对应的下架、恢复或通知任务同事务提交。
- `growth`：持有私人资源知识库、阅读进度、书签、路线关联、检索条件、只读陪伴简报和提醒偏好/设备注册；一条资源可原子转换为带首项行动的私人 Journey，资源关联本身是转换幂等边界；创建使用用户级 `Idempotency-Key`，所有关联路线均校验归属。
- `user-event`：最多 100 条批量事件、UUID 与时间校验、`event_id` 幂等；带推荐或搜索请求的事件按来源分别经 Recommend Main / Search Main 批量核验，无法核验的临时降级为无归因反馈；事件和 Outbox 同事务写入，`bookway-outbox-relay` 负责 Kafka 重试、指数退避与死信。

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
| `0005_interaction_status.sql` | interaction-status | 点赞/收藏状态 |
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
| `0029_comment_reply_depth.sql` | comment | 评论回复层级限制 |
| `0030_account.sql` | account | 账户公开资料 |
| `0031_commercialization.sql` | advertising / commerce | 广告活动/预算/事件、商品/SKU、库存预占和订单状态 |
| `0054_bbs_creator_message.sql` | community-creator / community-message | 创作者经营档案、一对一会话、私信、未读状态与私信意愿 |
| `0055_bbs_message_moderation.sql` | community-message | 私信举报审核队列、举报幂等键与发送者限制 |
| `0056_bbs_message_notification_delivery.sql` | community-message | 私信与通知任务的同事务 Outbox、租约投递索引 |
| `0057_comment_report_appeal.sql` | comment | 评论举报/申诉队列、幂等键和稳定审核游标索引 |

```bash
docker compose -f deploy/docker-compose.yml up -d
```

该命令启动 PostgreSQL 16、Redis 7、Redpanda、OpenSearch 2.17、MinIO 和 Prometheus，并在 Redpanda 健康后幂等创建 12 分区的 `bookway.domain-events.v1` Topic。复制 `.env.example` 中需要的变量并显式设置 `STORAGE_MODE=postgres`；业务服务不会自动执行迁移。

```bash
DATABASE_URL=postgres://bookway:bookway-local-only@127.0.0.1:5432/bookway \
cargo run -p bookway-db-migrate
cargo run -p bookway-outbox-relay
cargo run -p bookway-reminder-dispatcher
cargo run -p bookway-push-delivery-dispatcher
cargo run -p bookway-community-notification-dispatcher
cargo run -p bookway-direct-message-notification-dispatcher
cargo run -p bookway-route-participation-reconciler
cargo run -p bookway-appeal-notification-dispatcher
cargo run -p bookway-content-report-restriction-dispatcher
cargo run -p bookway-search-indexer
cargo run -p bookway-search-index-outbox-recovery
cargo run -p bookway-search-index-reconcile
# See bookway/bbs-search/job/README.md for the shadow-write, rebuild, reconcile, alias-switch rollout.
```

## 本地运行

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### Git 提交前契约边界检查

微服务实现 crate 只能作为服务二进制运行；跨服务依赖必须使用对应的
`bookway-<service>-api` 契约 crate。首次克隆仓库后执行一次：

```bash
git config core.hooksPath .githooks
```

提交时会运行 `scripts/check-service-implementation-dependencies.sh`。也可手动执行该脚本；
GitHub Actions 会运行相同检查，因此 `--no-verify` 不会绕过 CI。

依次启动：

```bash
cargo run -p bookway-bbs
cargo run -p bookway-bbs-creator
cargo run -p bookway-bbs-message
cargo run -p bookway-direct-message-notification-dispatcher
cargo run -p bookway-account
cargo run -p bookway-bbs-link
cargo run -p bookway-bbs-search
cargo run -p bookway-comment
cargo run -p bookway-interaction-status
cargo run -p bookway-recommend-main
cargo run -p bookway-bbs-feed
cargo run -p bookway-user-event
cargo run -p bookway-search-main
cargo run -p bookway-media
cargo run -p bookway-content-audit
cargo run -p bookway-feedback
cargo run -p bookway-feature-main
cargo run -p bookway-recommend-recall
cargo run -p bookway-recommend-rank
cargo run -p bookway-ad-center
cargo run -p bookway-ad-recall
cargo run -p bookway-ad-rank
cargo run -p bookway-ad-main
cargo run -p bookway-mall
cargo run -p bookway-mall-inventory
cargo run -p bookway-mall-order
cargo run -p bookway-growth
cargo run -p bookway-gateway
```

主要环境变量：

| 服务 | 监听变量 | 上游变量 |
| --- | --- | --- |
| gateway | `GATEWAY_ADDR` | `ACCOUNT_GRPC_URL`、`GROWTH_GRPC_URL`、`BBS_FEED_GRPC_URL`、`SEARCH_MAIN_GRPC_URL`、`USER_EVENT_GRPC_URL`、`BBS_LINK_GRPC_URL`、`BBS_GRPC_URL`、`BBS_CREATOR_GRPC_URL`、`BBS_MESSAGE_GRPC_URL`、`COMMENT_GRPC_URL`、`INTERACTION_STATUS_GRPC_URL`、`MEDIA_GRPC_URL`、`CONTENT_AUDIT_GRPC_URL`、`FEEDBACK_GRPC_URL` 、`PAYMENT_WEBHOOK_SECRET`（支付 webhook HMAC 验签密钥，签名输入为 `{provider}.{timestamp}.{raw_body}`，见「支付 Webhook 签名契约」；未配置时 `/payments/webhook/*` 返回 503，绝不无签名放行）、`PAYMENT_WEBHOOK_TOLERANCE_SECONDS`（webhook 时间戳容差秒数，默认 300） |
| account | `ACCOUNT_ADDR` | 无 |
| growth | `GROWTH_ADDR` | 无 |
| bbs | `BBS_ADDR` | 无 |
| bbs-creator | `BBS_CREATOR_ADDR`、`BBS_CREATOR_GRPC_ADDR` | PostgreSQL 创作者档案 |
| bbs-message | `BBS_MESSAGE_ADDR`、`BBS_MESSAGE_GRPC_ADDR` | `BBS_GRPC_URL`、`CONTENT_AUDIT_GRPC_URL`、PostgreSQL 会话/私信/举报/通知 Outbox；持久化模式下审核缺失即拒绝启动 |
| recommend-main | `RECOMMEND_MAIN_ADDR` | `BBS_GRPC_URL`、`INTERACTION_STATUS_GRPC_URL`、`FEATURE_MAIN_GRPC_URL`、`RECOMMEND_RECALL_GRPC_URL`、`RECOMMEND_RANK_GRPC_URL` |
| bbs-link | `BBS_LINK_ADDR`、`BBS_LINK_GRPC_ADDR` | `CONTENT_AUDIT_GRPC_URL` |
| comment | `COMMENT_ADDR`、`COMMENT_GRPC_ADDR` | `CONTENT_AUDIT_GRPC_URL`（持久化模式缺失时评论 fail-closed） |
| bbs-search | `BBS_SEARCH_ADDR` | `BBS_LINK_GRPC_URL` |
| interaction-status | `INTERACTION_STATUS_ADDR` | 无（由 Gateway 先校验内容） |
| bbs-feed | `BBS_FEED_ADDR` | `RECOMMEND_MAIN_GRPC_URL` |
| user-event | `USER_EVENT_ADDR` | `RECOMMEND_MAIN_GRPC_URL`、`SEARCH_MAIN_GRPC_URL`、PostgreSQL + Transactional Outbox，由 Relay 发布 Kafka |
| route-participation-reconciler | 无监听端口 | `DATABASE_URL`、`BBS_GRPC_URL`、`ROUTE_RECONCILE_*` |
| reminder-dispatcher | 无监听端口 | `DATABASE_URL`、`REMINDER_DISPATCH_BATCH_SIZE`、Outbox Relay |
| push-delivery-dispatcher | 无监听端口 | `DATABASE_URL`、`PUSH_DELIVERY_GATEWAY_URL`、`PUSH_DELIVERY_*` |
| community-notification-dispatcher | 无监听端口 | `DATABASE_URL`、`GROWTH_GRPC_URL`、`COMMUNITY_NOTIFICATION_*` |
| direct-message-notification-dispatcher | 无监听端口 | `DATABASE_URL`、`GROWTH_GRPC_URL`、`DIRECT_MESSAGE_NOTIFICATION_*` |
| appeal-notification-dispatcher | 无监听端口 | `DATABASE_URL`、`BBS_LINK_GRPC_URL`、`GROWTH_GRPC_URL`、`APPEAL_NOTIFICATION_*` |
| content-report-restriction-dispatcher | 无监听端口 | `DATABASE_URL`、`BBS_LINK_GRPC_URL`、`REPORT_RESTRICTION_*` |
| search-main | `SEARCH_MAIN_ADDR` | `BBS_SEARCH_GRPC_URL`、`BBS_LINK_GRPC_URL`、`BBS_GRPC_URL`（路线 join_count 水合；索引不存该计数，缺失时字段留空表示未读到事实，而非 0 人同行）、`KNOWLEDGE_CATALOG_GRPC_URL`、`FEATURE_MAIN_GRPC_URL`、`AD_MAIN_GRPC_URL`（特征与广告均可降级） |
| media | `MEDIA_ADDR` | `S3_ENDPOINT`、`S3_BUCKET`、`CDN_BASE_URL` |
| content-audit | `CONTENT_AUDIT_ADDR` | 审核规则与 PostgreSQL |
| feedback | `FEEDBACK_ADDR` | PostgreSQL；`user_feedback` 状态队列 |
| feature-main | `FEATURE_MAIN_ADDR` | `REDIS_URL`、PostgreSQL |
| recommend-recall | `RECOMMEND_RECALL_ADDR` | `BBS_LINK_GRPC_URL`、`RECALL_SOURCE_BLEND`（`balanced-v1` 或 `score-v1`）、`RECALL_SEMANTIC_BBS_SEARCH_URL` 与 `RECALL_SEMANTIC_KNOWLEDGE_CATALOG_URL`（两者同时配置才注册语义召回源：EmbedTexts 嵌入兴趣文本 + SearchSemantic 最近邻召回；任一缺失时该源不注册，其余来源照常工作） |
| recommend-rank | `RECOMMEND_RANK_ADDR` | `RECOMMEND_RANK_MODEL_VERSION`、`RECOMMEND_RANK_MODEL_ENDPOINT`（model_serving `/score`，LLM 三目标打分；不可用/未训练时自动降级启发式并如实上报 degraded）、`RECOMMEND_RANK_MODEL_ARTIFACT`（离线 LR 权重 JSON，见 `model-artifact.example.json`；配置后启动时校验加载，覆盖 endpoint） |
| ad-center | `AD_CENTER_ADDR` | PostgreSQL；活动和投放账本；`AD_CENTER_PACING_ENABLED`（默认开：线性日预算 pacing，1.5x 追赶余量） |
| ad-recall | `AD_RECALL_ADDR` | `AD_CENTER_GRPC_URL` |
| ad-rank | `AD_RANK_ADDR` | `AD_RANK_MODEL_VERSION`、`AD_RANK_CALIBRATION` |
| ad-main | `AD_MAIN_ADDR` | `AD_CENTER_GRPC_URL`、`AD_RECALL_GRPC_URL`、`AD_RANK_GRPC_URL`、`AD_MAIN_IMPRESSION_COOLDOWN_MS`（可选用户级曝光间隔，Redis 故障自动失效） |
| mall | `MALL_ADDR` | PostgreSQL 商品与 SKU 目录 |
| mall-inventory | `MALL_INVENTORY_ADDR` | PostgreSQL；`MALL_RESERVATION_TTL_SECONDS` |
| mall-order | `MALL_ORDER_ADDR` | `MALL_GRPC_URL`、`MALL_INVENTORY_GRPC_URL`、`MALL_PAYMENT_TTL_SECONDS`、`MALL_AFFILIATE_HOLD_DAYS`（分账冷静期天数，默认 7，0=立即 eligible；晋级由 expirer worker 驱动） |
| knowledge-catalog | `KNOWLEDGE_CATALOG_ADDR` | PostgreSQL；`RAG_VECTOR_ENABLED` + `RAG_EMBEDDING_ENDPOINT`/`RAG_EMBEDDING_MODEL`/`RAG_EMBEDDING_API_KEY`（OpenAI 兼容 embeddings，通常指向 bookway-py 的 model_serving；缺失时 EmbedTexts fail-closed、RAG 检索如实降级词法）、`BBS_LINK_GRPC_URL` |
| bbs-search/bg/bbs-indexer | 无监听端口 | `OPENSEARCH_URL`、`KNOWLEDGE_CATALOG_GRPC_URL`、`SEMANTIC_VECTOR_DIMS`（语义向量维度，写入索引后不可改，必须等于基模 hidden size；未设置时不建语义字段） |
| outbox-relay | 无监听端口 | `DATABASE_URL`、Kafka、`MALL_GRPC_URL`、`USER_EVENT_GRPC_URL`、`AD_CENTER_GRPC_URL`（订单带广告归因时回投广告转化；未配置时此类行进入死信并注明原因）、`OUTBOX_BATCH_SIZE` |
| bookway-py/bg/model_serving | `8110`（`MODEL_SERVING_PORT`） | `MODEL_NAME`、`MODEL_SOURCE`、`MODEL_DIR`、`MODEL_DEVICE`、`MODEL_CHECKPOINT_PATH`（静态 checkpoint）、`MODEL_REGISTRY_PATH`（训练侧原子发布的注册表，热加载优先于静态路径；两半契约：`scoring_head.pt` + `adapter/` 缺一拒绝服务）、`MODEL_MAX_BATCH`、`MODEL_MAX_INPUT_CHARS` |
| bookway-py/cronjob/rank_training | 无监听端口 | `DATABASE_URL`、`TRAINER_LLM_OUTPUT_DIR`、`TRAINER_REGISTRY_PATH`（通过门控后原子发布到 model_serving 热加载）、`TRAINER_LLM_MIN_AUC`（holdout AUC 门控，默认 0.55；不达标拒绝发布 exit 非零）、`TRAINER_LLM_*`、`TRAINER_LORA_*` |

全服务共享 `STORAGE_MODE`、`DATABASE_URL`、`SERVICE_AUTH_TOKEN`、`SERVICE_AUTH_REQUIRED`、`AUTH_REQUIRED`、`AUTH_JWT_SECRET`、`AUTH_JWKS_URL`、`AUTH_ISSUER`、`AUTH_AUDIENCE`、`AUTH_JWKS_CACHE_SECONDS`、`HTTP_CONNECT_TIMEOUT_MS` 和 `HTTP_REQUEST_TIMEOUT_MS`。Gateway 的 `CORS_ALLOWED_ORIGINS` 必须列出允许访问 Web API 的精确 `http(s)` Origin，拒绝通配符、路径和自定义 scheme；默认值仅供本机 Expo Web 调试。Redis 连接和命令预算分别由 `REDIS_CONNECT_TIMEOUT_MS`（默认 1000ms）与 `REDIS_COMMAND_TIMEOUT_MS`（默认 100ms）控制，缓存或限流 Redis 故障时会告警并 fail-open。生产环境必须启用两种鉴权：App 的 Bearer JWT 只在 Gateway 解析；配置 `AUTH_JWKS_URL` 时使用 OIDC/JWKS，按 TTL 缓存并在未知 `kid` 时立即刷新，`AUTH_ISSUER`/`AUTH_AUDIENCE` 可选约束；未配置 JWKS 时仅用于本地联调的 HS256 `AUTH_JWT_SECRET`。非 Gateway 服务的业务端点只接受带服务令牌的内部请求，并信任 Gateway 注入的 `x-user-id`。健康、就绪和指标端点不要求服务令牌。

如果默认端口被占用，可整组使用 `18080-18090` 并显式设置全部 URL；客户端将 `EXPO_PUBLIC_API_URL` 指向新的 Gateway 地址。

### 支付 Webhook 签名契约

`POST /payments/webhook/{provider}` 的签名覆盖 **provider 路径段、发送时间戳与原始请求体的拼接**：

- `x-payment-signature` = hex(HMAC-SHA256(key=`PAYMENT_WEBHOOK_SECRET`, message=`"{provider}." + x_payment_timestamp + "." + raw_body`))。provider 即 URL 路径段原样字节，时间戳取 `x-payment-timestamp` 头原样字节（Unix 秒），网关在服务端拼接 `.` 分隔符后整体验签；针对某 provider 抓取的签名无法在另一 provider 的端点重放。
- `x-payment-timestamp` 必须存在且可解析，验签通过后再做新鲜度校验：与服务端时钟之差超过 `PAYMENT_WEBHOOK_TOLERANCE_SECONDS`（默认 300 秒，对称窗口）即返回 401。时间戳已进入签名输入，因此抓包者无法靠改时间戳把旧投递刷新到窗口内；新鲜度检查放在验签之后，未认证的调用方问不出服务端时钟。
- 请求体为 JSON，必须携带非空 `payment_reference`；mall-order 以该流水号驱动与 `Pay` 相同的幂等状态机。
- 确认到达晚于 `MALL_PAYMENT_TTL_SECONDS` 时，订单如实进入 `paid_after_expiry` 终态（不自动履约、不生成分账，运营决定退款或补履约），provider 不会陷入 failed_precondition 重试循环。
- `PAYMENT_WEBHOOK_SECRET` 未配置时端点整体返回 503，绝不无签名放行。
- 该方案与 Stripe 式 `t=…,v1=…` 构造同形（签名覆盖时间戳+原文、对称容差窗口）。若接入 RSA 验签的 PSP（微信支付 v3 的 `RSA-SHA256(timestamp\nnonce\nbody\n)`、支付宝 RSA2），需按 provider 分派到各自的验签实现，而不是把house HMAC 当通用协议使用。

## 已实现的生产能力

- SQLx 连接池和按服务所有权拆分的 PostgreSQL Dao，写路径使用事务与幂等键。
- Redis 全局限流和特征缓存；Redis 故障 fail-open 并记录告警。
- BBS 关系/可见性上下文 Redis 缓存、跨实例刷新租约、写后版本失效和安全 fail-closed 回退。
- Transactional Outbox、Kafka/Redpanda Relay、`SKIP LOCKED`、退避和死信状态。
- OpenSearch CJK 物理/影子双写索引、可恢复全量重建、逐版本只读对账、读别名原子发布和检索降级。
- Gateway 媒体 API、MinIO/S3 预签名直传、对象大小/MIME 完成校验、私有 pending 元数据和 CDN 地址。
- `feature-main -> recommend-rank` 在线调用，推荐模型失败时保留启发式得分并标记降级。
- `feature-main` 从近 90 天行为构造领域/作者亲和度、重复曝光疲劳和直接负反馈候选特征；`recommend-rank` 在用户级特征之外执行候选级个性化。
- `hide` 负反馈闭环、局部窗口多样性重排、模型/实验诊断元数据和来源降级传播。
- 动态搜索联想、查询统计、查询绑定游标，以及行动留痕与服务端周回望聚合。
- 基于进行中路线、真实行动状态和精确安排时间的只读陪伴简报；建议由用户显式采纳，服务不会隐式改写计划。
- 用户举报与作者申诉的幂等接入、PostgreSQL 人工队列、稳定续页、审核认领/结案、角色受限的下架与恢复动作；举报下架、获准内容恢复与终态通知均通过事务任务、租约调度与幂等写入可靠收敛。
- 私信接收者举报、可信审核队列、终态审核保护与持久化发送者限制；普通举报回执不暴露私信正文，原始正文只在受限审核 RPC 中可读。
- 评论回复、弱网写入幂等、稳定游标分页、移动端失败草稿恢复和加载更多；点赞、评论和关注在 Gateway 已解析接收者后经租约 Worker 与幂等来源键可靠投递社区收件箱。
- 私人资源知识库、阅读进度与书签持久化，以及基于真实作者 ID 的关注关系和独立关注流。
- 公共路线参与持久化、跨端加入状态恢复、真实同行人数，以及推荐/搜索的批量上下文补全。
- 服务端幂等路线加入编排；并发、超时或客户端重启后的重试会复用同一私人 Journey。
- 路线参与 Saga 使用版本化期望状态、`SKIP LOCKED` 租约协调器和 BBS 原子版本门禁；同步双写失败、乱序完成及进程崩溃均可自动收敛。
- 热门路线参与使用用户级并发控制和 64 分片事务计数，重复/乱序命令不重复计数，批量上下文不再扫描完整参与事实。
- `infra/runtime` tonic 客户端容错层（连接超时、单调用期限、幂等重试预算与熔断半开），全部服务间 gRPC 客户端统一接入；`pkg/cache` 提供版本化缓存 + miss 锁 + 刷新租约公共库。
- 推荐链路：服务端硬曝光频控（Redis 计数加速、Postgres 权威）、独立粗排打分、`MultiObjectivePredictor` 多目标精排接口（pCTR/pCVR/pWEGU，权重按实验桶版本化，远程模型未部署时启发式兜底并标记降级）、`pkg/commercial-mix` 密度驱动多槽 eCPM 混排（推荐与搜索共用）与冷启动匿名页短 TTL 防击穿缓存。
- 搜索与知识库：Node/Gear 实体化语义搜索类型，`knowledge-catalog` 可插拔 embedding provider（OpenAI-compatible `/embeddings`）+ RAG 向量接线，provider 未配置时词法兜底行为不变；`embedding-builder` job 扫描待嵌入资源并经 `UpsertRagEmbedding` 写入。
- 商城闭环：商品课程类目（`product_kind` + `course_resource_id` 校验链）、商家库存 `SetStock` 网关联通与缓存失效、结算打款前端接通、分账 `ReverseAffiliate` 幂等路径、购买归因事务 Outbox 化（退避 + 死信）。
- 广告闭环：Redis 三键频控预过滤（campaign×user / campaign×global / 跨 campaign 用户日总，PG 行锁裁决权威不变）、geo/device 硬定向 fail-closed 过滤、ecpm-v3 Beta 后验校准（`AD_RANK_CALIBRATION=false` 一键回退静态合同）、可选用户级曝光间隔 pacing（fail-open）。
- 广告平台真实数据面：每日账本 `DeliveryReport` 与交付护栏 API（上限写入仅限平台 admin，广告主只读透明），广告主后台报表/创意/护栏全部读写真实网关。
- 社区补全：粉丝 keyset 分页列表与社交计数（`GET /v1/users/{id}/followers|social-stats`，0080 覆盖索引），同行者列表 `GET /v1/routes/{id}/peers` 经 fail-closed 可见性过滤双向拉黑/静音。
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
