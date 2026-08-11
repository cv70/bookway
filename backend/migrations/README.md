# 数据库迁移基线

迁移按微服务数据所有权拆分：`bbs-link`、`bbs`、`bbs-search`、`recommend-main`、`commonlikestatus`、`comment`、`user-event` 和 `growth`。生产环境由 `bookway-db-migrate` 在发布流水线中执行，业务服务启动时不会自动改表；当前目录是可审查的迁移基线。

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

`STORAGE_MODE=postgres` 时服务使用 SQLx PostgreSQL Repository；`STORAGE_MODE=memory` 仅用于无依赖本地演示。生产部署必须先运行 `cargo run -p bookway-db-migrate`，再启动业务服务并确认连接池、Outbox/Worker 和迁移版本健康。
