# 数据库迁移基线

迁移按微服务数据所有权拆分：`bbs-link`、`bbs`、`bbs-search`、`recommend-main`、`interaction-status`、`comment`、`user-event`、`growth` 和 `content-audit`。生产环境由 `bookway-db-migrate` 在发布流水线中执行，业务服务启动时不会自动改表；当前目录是可审查的迁移基线。

| 文件 | 服务 | 主要表 |
| --- | --- | --- |
| `0001_content.sql` | bbs-link | 内容、媒体、主题、幂等键 |
| `0002_bbs.sql` | bbs | 关注、拉黑、静音关系 |
| `0003_search.sql` | bbs-search | 搜索文档、查询统计 |
| `0004_feed.sql` | recommend-main | 曝光、推荐事件 |
| `0005_interaction_status.sql` | interaction-status | 点赞、收藏状态 |
| `0006_comment.sql` | comment | 评论、审核状态 |
| `0007_user_event.sql` | user-event | 曝光、点击等行为事件与幂等键 |
| `0008_growth.sql` | growth | 路线与行动 JSONB 读写模型 |
| `0009_outbox.sql` | data-platform | Transactional Outbox、服务审计日志 |
| `0010_media_audit_features.sql` | platform | 媒体资产、审核记录、用户特征、模型版本 |
| `0011_feedback_reviews_search.sql` | recommendation / growth / search / trust-safety | 负反馈、行动留痕、动态搜索联想与社区举报队列 |
| `0012_knowledge_companionship.sql` | growth | 私人资源知识库、阅读进度、书签、路线关联和幂等写入 |
| `0013_route_participation.sql` | bbs | 公共路线参与、私人路线关联、活跃同行人数 |
| `0014_idempotent_route_join.sql` | growth | 公共路线来源唯一绑定和跨服务加入重试去重 |
| `0015_route_participation_reconciliation.sql` | growth / bbs | 版本化参与意图、后台协调租约和 BBS 乱序写保护 |
| `0016_route_participation_sharded_counts.sql` | bbs | 64 分片活跃同行计数、存量回填和事务触发器 |
| `0017_user_event_personalization.sql` | user-event / recommendation | 补齐事件类型约束、候选级个性化查询索引 |
| `0018_comment_scale.sql` | comment | 评论写入幂等键与稳定游标分页索引 |
| `0019_action_scheduling.sql` | growth | 行动精确安排瞬间、IANA 时区与本地日期索引 |
| `0020_reminder_delivery.sql` | growth | 提醒偏好、推送设备、去重投递记录和行动安排版本 |
| `0021_notification_inbox.sql` | growth | 用户通知收件箱、未读状态和生产端幂等键 |
| `0022_search_sessions.sql` | bbs-search | 短期搜索会话和稳定游标状态 |
| `0023_comment_moderation.sql` | comment | 公开已发布评论的审核读取索引 |
| `0024_content_appeals.sql` | trust-safety | 内容作者申诉、审核队列与幂等键 |
| `0025_content_appeal_owner_lookup.sql` | trust-safety | 作者私有申诉历史的游标查询索引 |
| `0026_content_appeal_notification_jobs.sql` | trust-safety | 终态申诉的可恢复作者收件箱投递任务 |
| `0027_content_report_restriction_jobs.sql` | trust-safety | 接受举报后的可恢复内容下架任务 |
| `0028_search_main_sessions.sql` | search-main | 多路搜索召回的短期会话与稳定公开游标 |
| `0029_comment_reply_depth.sql` | comment | 评论树层级回填与受限回复深度 |
| `0030_account.sql` | account | 账户公开资料 |
| `0031_commercialization.sql` | advertising / commerce | 广告投放（曝光、点击及服务端转化事实）、商品目录、库存预占、订单支付状态 |
| `0032_commercialization_reliability.sql` | commerce | 支付渠道流水号的跨订单唯一约束 |
| `0033_feedback.sql` | feedback | 用户产品反馈、状态、处理说明与幂等键 |
| `0034_content_index_outbox.sql` | bbs-link / bbs-search | 内容搜索投影的事务性 Outbox、租约与存量回填 |
| `0035_entry_publication_jobs.sql` | growth / bbs-link | 私人记录到公开行记的可恢复发布任务与旧数据状态回填 |
| `0036_media_processing_and_content_asset_refs.sql` | media / bbs-link | 媒体处理租约队列、公开内容的受信资产引用与顺序约束 |
| `0037_search_exposure_attribution.sql` | search-main | 搜索结果页曝光与可验证归因 |
| `0038_user_event_negative_feedback.sql` | user-event / recommendation | 结构化隐藏反馈原因与特征语义 |
| `0039_recommendation_evaluation.sql` | recommend-main / user-event | 版本化曝光身份、可信归因读取索引与离线评估快照 |
| `0040_search_query_rewrite_versions.sql` | search-main | 版本化查询改写词典、原子活动指针与曝光审计标签 |
| `0041_search_evaluation.sql` | search-main / user-event | 搜索改写版本的匿名离线评估快照 |
| `0042_search_index_recovery.sql` | bbs-link / bbs-search | 搜索投影死信报告与受控重排审计 |
| `0043_search_index_reconciliation.sql` | bbs-search | 搜索投影版本/可见性对账的持久运行、检查点和最终结论 |
| `0044_push_delivery_dispatch.sql` | growth | 提醒 Provider 投递租约、退避重试、失败状态与领取索引 |
| `0045_community_notification_jobs.sql` | gateway / growth | 跨服务互动已解析收件人的可靠社区收件箱投递任务 |
| `0046_knowledge_content_sources.sql` | growth / gateway | 社区内容到私有知识库的稳定来源去重索引 |
| `0047_following_timeline.sql` | bbs-link / recommendation | 关注作者批量时间流的公开内容读取索引 |
| `0048_action_idempotency.sql` | growth | 用户创建行动的幂等键与弱网重试去重 |
| `0049_user_event_knowledge_signal.sql` | user-event / recommendation | 私有知识收集的高意图信号约束 |
| `0050_entry_idempotency.sql` | growth | 复盘/行记创建的幂等键与发布任务去重 |
| `0051_journey_idempotency.sql` | growth | 私人路线与首项行动的幂等创建快照 |
| `0052_content_publish_idempotency.sql` | bbs-link | 内容发布审核结果的幂等响应快照 |
| `0053_comment_moderation_queue.sql` | comment | 待审评论分页索引与人工审核决定审计 |
| `0058_weekly_reviews.sql` | growth | 用户确认的周复盘、不可变指标快照和同周文字编辑 |
| `0059_content_milestones.sql` | bbs-link | 公开路线阶段成果的约束与路线快照事实 |
| `0060_questions.sql` | bbs-link | 问题内容类型和采纳回答事实约束 |
| `0061_search_query_history.sql` | bbs-search | 登录用户的 90 天搜索历史建议和按用户最近时间索引 |
| `0062_feature_snapshots.sql` | feature-main | 可回放的用户行为特征快照、时间窗口、版本、TTL 和来源血缘 |
| `0063_public_resources.sql` | knowledge-catalog | 公开书籍、课程、工具、文章和播客目录及许可/版本/引用元数据；URL 唯一（管理员 UpsertPublicResource 写入，无种子数据） |
| `0064_contextual_commerce.sql` | mall / mall-order | 路线行动节点 Offer、创作者佣金与订单归因快照 |
| `0065_ad_auction_frequency.sql` | ad-center / ad-rank | pCTR/pCVR 预估、eCPM 竞价输入与全局曝光频控 |
| `0066_unified_resource_search.sql` | knowledge-catalog / search-main | 公共资源统一搜索投影、触发刷新与目录检索索引 |
| `0067_route_node_resource_attachments.sql` | knowledge-catalog | 绑定公开路线行动节点及其场景装备的资源挂载、RAG 检索集合预留、幂等创建和软归档 |
| `0068_mall_merchant_ownership.sql` | mall | 商品与节点 Offer 的商家所有权和管理读取索引 |
| `0069_ad_scene_equipment.sql` | ad-center | 广告路线节点绑定的场景装备字段与投放索引 |
| `0070_ad_decision_scene_equipment.sql` | ad-center | 广告决策与曝光回执的场景装备上下文绑定 |
| `0071_merchant_order_settlement.sql` | mall-order | 商家订单履约与创作者分账结算 |
| `0072_route_node_resource_embeddings.sql` | knowledge-catalog | 节点作用域 RAG 向量索引及模型边界（已移除原 `WHERE FALSE` 的死种子语句，仅保留建表与索引） |
| `0073_route_completion_event_index.sql` | feature-main | 路线完成度同用户加入事实校验的事件索引 |
| `0074_order_payment_processing.sql` | mall-order | 支付处理中间状态与取消/过期竞态保护 |
| `0075_rag_embedding_builder_state.sql` | knowledge-catalog | RAG 嵌入构建器的重试簿记列（是否待嵌入以向量行缺席为准） |
| `0076_mall_product_kinds.sql` | mall | 商品类目（physical/course/resource_pack）与知识资源绑定列 |
| `0077_purchase_event_outbox.sql` | mall-order | 支付同事务入队的购买归因 Outbox 与死信/退避状态机 |
| `0078_ad_delivery_guardrails.sql` | ad-center | 跨 campaign 用户日曝光上限等投放护栏配置（缺行回退代码默认值） |
| `0079_ad_campaign_geo_device.sql` | ad-center | 活动地域/设备定向数组（空=不限，fail-closed 匹配）与 GIN 索引 |
| `0080_bbs_follower_pages.sql` | bbs | 粉丝 keyset 分页的部分覆盖索引（follow+未删按时间/ID 降序） |
| `0081_feed_exposure_item_objectives.sql` | recommend-main | `feed_exposure_items` 增加 `p_ctr`/`p_cvr`/`p_wegu` 列：记录排序服务对本次曝光的三目标预估，供校准与实验评估使用 |
| `0082_user_event_route_fork.sql` | user-event | `user_events` 事件类型约束增加 `route_fork`（Gateway 在 fork 公开路线后写入，稳定键=每个 fork 实例） |
| `0083_ad_conversion_events.sql` | ad-center | 放宽 `ad_delivery_events` 的事件约束以接受服务端 `conversion`，并为 `ad_campaigns`/`ad_campaign_daily_stats` 增加 `conversions` 列：转化事件记账（要求已受理曝光回执，不计费），供 ad-rank CVR 校准 |
| `0084_affiliate_settlement_hold.sql` | mall-order | 分账冷静期部分索引：`pending` 行按 `eligible_at` 供 expirer 晋级扫描；窗口内退款直接作废分账 |
| `0085_feed_exposure_feature_snapshot.sql` | recommend-main | `feed_exposure_items` 增加 `feature_snapshot` JSONB：记录排序时刻的命名特征，作为唯一无泄漏训练输入 |
| `0086_mall_order_paid_after_expiry.sql` | mall-order | 订单状态约束增加 `paid_after_expiry`：TTL 过期后 provider 回调确认的收款如实落为独立终态，不自动履约、不生成分账 |
| `0087_mall_order_ad_attribution.sql` | mall-order | `mall_orders`/`purchase_event_outbox` 增加可空 `ad_request_id`/`ad_campaign_id`：下单携带广告决策上下文，支付成交后由 outbox-relay 服务端回投 ad-center 转化（客户端信标路径已拒绝 conversion 类型） |

`STORAGE_MODE=postgres` 时服务使用 SQLx PostgreSQL Dao；`STORAGE_MODE=memory` 仅用于无依赖本地演示。生产部署必须先运行 `cargo run -p bookway-db-migrate`，再启动业务服务并确认连接池、Outbox/Worker 和迁移版本健康。`0016` 在持有参与事实表 DDL 锁的同一事务中安装计数触发器并回填，因此旧 BBS 实例的后续写入也会维护分片，可先迁移再滚动升级服务。`0019` 不为旧行动编造精确时间；旧数据保留原有本地日期行为，只有新建或显式改期的行动会写入精确安排瞬间和时区。`0020` 为精确安排引入版本号；改期或完成会取消旧的排队投递，而提醒调度器以 `(action_id, schedule_revision, channel, device_id)` 去重创建通知命令。`0021` 使用 `(kind, source_id)` 将生产端重试折叠为唯一收件箱项，并提供按用户的稳定游标和未读索引。`0026` 让终态申诉、恢复公开命令和作者收件箱任务原子落库；`0027` 将接受举报后的下架任务与决议同事务提交，两个调度器都由租约负责重试，`dead` 状态必须纳入运营告警与人工补偿。`0032` 在启用支付回调前要求确认历史数据不存在重复的 provider 流水号；迁移后同一流水号只能结算一个订单。`0034` 会把既有内容回填为待投影任务；上线后应启动 `bookway-search-indexer`，并由 `bookway-search-index-outbox-recovery` 报告死信；只有明确操作者和原因的 `requeue_dead` 运行才能将死信安全重排。`0035` 保持旧客户端的已发布标记而不重复创建未知原帖；新记录的 `entry_publication_jobs.status = 'dead'` 也必须进入告警，并由用户显式重试或运营补偿处理。`0036` 将 `complete_upload` 后的资产放入处理队列，只有 `bookway-media-processor` 验证完成的 `ready` 资产能进入 BBS Link；必须对 `media_processing_jobs.status = 'dead'` 告警。`0039` 会将每次已持久化曝光对应的模型版本与实验桶保留下来，并建立只含请求归因事件的读取索引；`bookway-recommendation-evaluator` 必须使用已完成标签窗口的固定时间范围，它的快照是观察性评估记录，不得用作未经验证的训练或自动发布依据。`0040` 为 Search Main 增加受限、版本化的查询改写词典和原子活动指针；词典切换前必须完成离线检查，调用 `activate_search_query_rewrite` 后各实例会热刷新，旧分页会话保持创建时的改写版本。`0041` 为 `bookway-search-evaluator` 保存按改写版本和结果类型聚合的观察性指标；每次运行都必须使用完整标签窗口，样本不足的快照不能作为词典升级依据。`0043` 持久化搜索索引对账的受限检查点、聚合差异、Outbox 未投递数量和最终健康结论；同一未完成运行由租约串行恢复，只有完成、全量且 Outbox 已清空的运行才可能标记为健康。检查点包含内部内容 ID，必须仅向受控运维数据库访问者开放。`0044` 将提醒 Provider 投递从收件箱创建中分离为可租约领取的 Worker：发送前必须复核行动版本和设备，Provider 以 delivery ID 幂等，超时退避、失效设备撤销、终态失败均可审计。`0045` 让 Gateway 在互动后将已解析接收者和固定来源键写入可靠的社区通知任务，Worker 以租约投递 Growth；它不能跨服务消除互动已提交但 Gateway 尚未入队的窗口，且 `dead` 与入队失败日志必须由运营监控处理。`0046` 为知识资源增加可选社区内容来源标识和用户级唯一索引；它不回填正文或媒体，收集公开内容只保存受控元数据与原内容引用，内容可见性始终由 BBS Link 决定。`0047` 为关注时间流提供 `(author_id, created_at DESC, id DESC)` 的已公开内容读取索引；推荐召回只能使用 BBS 派生的关注作者集合，空关注集合必须返回空流而非回退到全局推荐。`0048` 为显式创建行动增加用户级幂等键；同一键配合相同动作返回已有行动，键与动作内容不匹配则返回冲突，重复计划自动物化的后继行动不使用该键。`0049` 使 PostgreSQL 的 `user_events` 枚举约束接受 Gateway 产生的 `save_knowledge` 事件；应用、特征和离线评估的高意图语义由此在生产存储中保持一致。`0050` 以用户级幂等键折叠复盘/行记创建的弱网重试；公开行记只会产生一条持久发布任务，而已发布后的同键重试仍会返回原始记录。

`0051` 为显式创建私人 Journey 保存规范化的首次创建快照。它与运行中 Journey 及行动的可变 payload 分开存储，所以用户随后改名、改期、完成首项行动后，丢失响应的同键重试仍只会返回原 Journey；同键不同的初始阶段或首项行动会被拒绝。`0052` 为内容发布操作保存审核完成时的内容快照；弱网重试会返回该原始结果，绝不会再次审核、递增版本或因之后的编辑而误返回当前状态。`0053` 为评论人工审核提供只扫描待审行的稳定索引和首个终态决定的审计记录；审核服务必须先部署兼容代码再将人工工作台流量切入该端点。`0058` 为每个用户的每个自然周保存首次确认的回望摘要；并发创建由唯一键收敛，同周的再次保存只能更新用户写下的结论和下周重点，不能改写指标、建议或创建时间。
`0061` 为登录搜索提供个人历史建议；查询文本按用户级稳定键去重，建议读取只在 90 天窗口内按用户返回，匿名请求不写入该表。全局搜索词仍需达到聚合阈值后才会作为公共建议。
`0066` 将公共资源的标题、摘要、提供方、引用和主题维护为目录内搜索投影；Search Main 的统一资源召回只读取 Knowledge Catalog 的已发布目录事实，不复制资源正文或绕过目录授权。
`0067` 把路线行动节点与公共资源的挂载关系收敛在 Knowledge Catalog 限界上下文中，并要求每条挂载声明该行动节点已公开声明的场景装备。挂载记录只引用公开资源、公开路线节点标识和装备上下文，不复制路线树或资源正文；`embedding_collection` 为后续 RAG/AI 行动指南提供稳定检索集合名。

`0074` 为订单支付确认增加 `payment_processing` 中间状态。订单先原子进入处理中，再确认库存并完成支付；取消与过期不会抢占处理中订单，支付重试可继续同一支付号。

`0077` 让购买归因从 best-effort gRPC 升级为事务 Outbox：标记已支付的同事务写入 `purchase_event_outbox`（每单一行，仅含路线归因的场景订单），`bookway-outbox-relay` 在投递时解析 Offer 路线并以确定性 UUIDv5 键调用 user-event（重放天然幂等），瞬时失败按指数退避、永久失败与超限死信均落入 `dead` 需运营告警。迁移会将既有已支付场景订单一次性回填入队，重复执行不产生重复事件。

`0078` 将广告平台此前仅存在于浏览器演示态的“单用户全局日曝光上限”落为服务端护栏表 `ad_delivery_guardrails`（种子 8）。ad-center 的 `RecordEvent` 在每次曝光受理时读取该值并在跨所有 campaign 的用户日总达标后拒绝；Redis 预过滤（`FrequencyGate`）仅为加速器，计数漂移或 Redis 不可用时自动退回 SQL 权威裁决，删除护栏行不会静默关闭上限。

`0079` 为广告活动增加 `geo_regions`/`device_os` 定向数组。语义是硬过滤且 fail-closed：空数组不限，非空数组要求请求投送上下文携带匹配值；平台观察不到上下文时只有未限定活动可参与。GIN 索引支撑数组过滤；定向 slug 全链路小写归一（如 `cn-bj`、`ios`），网关当前从 User-Agent 派生设备维度，地域维度留待可靠来源接入。

`0086` 让 TTL 过期后到达的支付确认不再永久失败：provider 回调（或重试支付）命中已 `expired` 且带有已声明支付号的订单时，订单如实进入独立的 `paid_after_expiry` 终态。该状态不自动履约、不生成分账、不入购买归因 Outbox——库存已在过期时释放，运营按单决定退款或补履约；幂等重放停留在同一状态。

`0085` 打通训练闭环的输入侧：recommend-rank 对每个候选记录其排序所用特征集（与 predictor 契约同集合），离线训练（`backend/bookway-py/cronjob/rank_training`，PyTorch）直接以该快照拟合，禁止回读当前聚合（防时间泄漏）；时间切分 holdout 输出 logloss/AUC，正样本不足拒绝出产物。

`0084` 为创作者分账引入退款冷静期：已支付订单的分账以 `pending` 写入（`MALL_AFFILIATE_HOLD_DAYS`，默认 7 天），`mall-order-expirer` 每轮调用 `PromoteAffiliateSettlements` 将到期行晋级为 `eligible`；冷静期内退款会把 pending 分账直接置为 `reversed`，不再出现先打款后追讨。`MALL_AFFILIATE_HOLD_DAYS=0` 保留旧的立即 eligible 行为。

`0083` 打通广告转化回流：`EVENT_TYPE_CONVERSION` 事件在已登记决策与已受理曝光的前提下入账（幂等、不计费），campaign 行与日统计各维护 `conversions`；`ad-rank` 的校准模型（`ecpm-v3`）据此对 pCVR 做与 pCTR 对称的 Beta 后验校准，静态口径不变。

`0082` 为行为事实增加 `route_fork`：fork 一条公开路线是强采用信号（含路线改写意图），事件 ID 按 fork 实例稳定可重放，`content_id` 归因到被 fork 的源路线；推荐特征与评估可直接消费该类型。

`0081` 把 recommend-rank 输出后即被丢弃的三目标预估（pCTR/pCVR/pWEGU）持久化到每条曝光明细；推荐评估与未来训练因此能按目标分组对比"当时预测"与"实际行为"，而不是只看融合分。旧数据按默认 0 回填，评估作业应过滤全零行或按迁移时间切分。

`0080` 为粉丝分页建立部分覆盖索引 `(target_user_id, created_at DESC, source_user_id DESC) WHERE edge_type='follow' AND deleted_at IS NULL`。`ListFollowers` 的 keyset 游标（`(created_at, source_user_id)` 行值比较）依赖该索引把每页收敛为一次有序索引扫描；0002 的通用目标索引保留给可见性上下文读取。
