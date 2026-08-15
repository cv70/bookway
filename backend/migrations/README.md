# 数据库迁移基线

迁移按微服务数据所有权拆分：`bbs-link`、`bbs`、`bbs-search`、`recommend-main`、`commonlikestatus`、`comment`、`user-event`、`growth` 和 `content-audit`。生产环境由 `bookway-db-migrate` 在发布流水线中执行，业务服务启动时不会自动改表；当前目录是可审查的迁移基线。

| 文件 | 服务 | 主要表 |
| --- | --- | --- |
| `0001_content.sql` | bbs-link | 内容、媒体、主题、幂等键 |
| `0002_bbs.sql` | bbs | 关注、拉黑、静音关系 |
| `0003_search.sql` | bbs-search | 搜索文档、查询统计 |
| `0004_feed.sql` | recommend-main | 曝光、推荐事件 |
| `0005_commonlikestatus.sql` | commonlikestatus | 点赞、收藏状态 |
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

`STORAGE_MODE=postgres` 时服务使用 SQLx PostgreSQL Repository；`STORAGE_MODE=memory` 仅用于无依赖本地演示。生产部署必须先运行 `cargo run -p bookway-db-migrate`，再启动业务服务并确认连接池、Outbox/Worker 和迁移版本健康。`0016` 在持有参与事实表 DDL 锁的同一事务中安装计数触发器并回填，因此旧 BBS 实例的后续写入也会维护分片，可先迁移再滚动升级服务。`0019` 不为旧行动编造精确时间；旧数据保留原有本地日期行为，只有新建或显式改期的行动会写入精确安排瞬间和时区。`0020` 为精确安排引入版本号；改期或完成会取消旧的排队投递，而提醒调度器以 `(action_id, schedule_revision, channel, device_id)` 去重创建通知命令。`0021` 使用 `(kind, source_id)` 将生产端重试折叠为唯一收件箱项，并提供按用户的稳定游标和未读索引。`0026` 让终态申诉、恢复公开命令和作者收件箱任务原子落库；`0027` 将接受举报和下架任务原子落库。两个调度器都由租约负责重试，`dead` 状态必须纳入运营告警与人工补偿。
