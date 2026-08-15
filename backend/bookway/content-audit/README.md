# content-audit 内容审核服务

内容审核服务负责文本规则、外部审核提供商结果归一化、风险分数、审核版本和人工复审状态。当前实现提供可运行的规则引擎，并将每次决策持久化到 `content_audits`；后续云审核或自研模型只需作为新的 provider 接入，不改变 `bbs-link` 状态机。

内部 gRPC `audit` 返回 `approved`、`reviewing` 或 `restricted`。高风险禁止词进入受限，中风险健康/广告内容进入复审，其余允许发布。`report` 以 `(reporter_id, Idempotency-Key)` 去重写入 `community_reports`。

人工审核使用内部的 `list_reports` 和 `review_report`：队列按 `(created_at, id)` 正序续页，可筛选 `pending`、`reviewing`、`resolved` 或 `rejected`。审核员可认领为 `reviewing`，或以不超过 1000 字的处置说明结案为 `resolved` / `rejected`；终态的相同重试保持幂等，任何不同的终态处置都会冲突，不会覆盖原决定。`SERVICE_AUTH_REQUIRED=true` 时，这两个方法必须携带 `x-service-token`，因此不能由 App 直接调用。

结案还可记录明确动作 `restrict_content`。终态举报与 `content_report_restriction_jobs` 任务会在同一 PostgreSQL 事务中写入；Gateway 会尝试低延迟下架，但失败只记录告警，`bookway-content-report-restriction-dispatcher` 会带服务令牌幂等重试，并在公开读取已不可用后确认完成。因此短暂跨服务故障不会让已接受的下架决定消失。

内容作者可通过 `appeal` 创建带幂等键的独立申诉；运营侧使用 `list_appeals`、`review_appeal` 处理。`list_appeals` 可按作者、内容和状态稳定续页；Gateway 为普通作者强制注入作者过滤，不会暴露审核队列或他人历史。获准的 `resolved + restore_content` 只能恢复此前受限的内容，拒绝和待审决定无法改变内容状态。申诉、原举报和处置决定分别持久化，任何后续操作都不会改写历史记录。

终态审核决定与 `content_appeal_notification_jobs` 任务会在同一 PostgreSQL 事务中写入。`bookway-appeal-notification-dispatcher` 以租约领取任务、指数退避重试，并带服务令牌幂等执行 `restore_content`；只有随后 `bbs-link.get_public` 确认内容已公开，才会用稳定来源键写入 Growth 私有收件箱。Gateway 的恢复调用只是低延迟快路径，失败不会回滚终态决定，Worker 会负责最终收敛。申诉 SLA、双人复核和多媒体审核仍需继续实现。默认监听 `8092`。
