# Comment 评论服务

## 职责

`comment` 持有评论正文、父子关系、审核状态和评论列表。帖子是否存在由 Gateway 在写入前通过 `bbs-link` 校验。

## 接口

- `GET /v1/posts/{post_id}/comments?limit=30&cursor=...`：按 `(created_at, id)` 稳定游标分页，单页最多 50 条。
- `POST /v1/posts/{post_id}/comments`：支持 `parent_id` 回复和 `Idempotency-Key` 弱网重试去重。
- `DELETE /v1/posts/{post_id}/comments/{comment_id}`：仅评论作者可幂等软删除自己的评论。
- 受信内部 gRPC：`list_moderation` 读取待审队列，`review` 以 `approve` 或 `restrict` 完成人审；客户端只能经 Gateway 的受角色保护审核入口访问。
- `POST /v1/posts/{post_id}/comments/{comment_id}/report`：当前可见评论的举报，必须携带 `Idempotency-Key`。Gateway 覆盖举报人和社交可见性集合；用户响应是无正文回执。
- `POST /v1/comments/{comment_id}/appeals`、`GET /v1/me/comment-appeals`：评论作者只能对自己当前受限且未删除的评论提交或查看申诉，提交必须携带 `Idempotency-Key`。
- `GET/PATCH /v1/moderation/comment-reports*`、`GET/PATCH /v1/moderation/comment-appeals*`：仅审核角色可访问的稳定游标队列和处置入口；完整评论上下文只在这些可信审核响应中出现。

服务拒绝空评论、超长评论、不属于同一帖子的父评论、超过三层的回复链，以及复用到不同请求的幂等键。根评论深度为 0，最多可创建三层回复；该限制在 Memory 和 PostgreSQL 仓储内同时执行，`0029_comment_reply_depth.sql` 会为历史数据回填层级，避免递归渲染和祖先查询被恶意深链放大。评论先以 `reviewing` 状态写入，再请求 `content-audit`：通过后才成为 `published`，受限内容保持 `restricted`；审核服务故障也保持待审。公开列表和回复父评论都只接受已发布评论，因此待审或受限正文不会泄漏。删除后的叶评论从公开列表移除；仍有可见后代的删除评论会以 `deleted` 匿名墓碑保留在链路中，正文、作者 ID 和原作者名不会泄漏，也不能继续被回复。墓碑同样服从作者可见性策略。新旧 gRPC 列表响应在滚动发布期间保持兼容。

面向用户的请求由 Gateway 从可信身份生成社交可见性集合，并覆盖传入的内部字段。评论仓储在稳定游标查询内先排除这些作者，避免后置裁剪产生空页或遗漏；创建回复也使用同一集合校验 `parent_id`，隐藏父评论与不存在或未发布的父评论返回相同结果。

创建 RPC 在保持旧版扁平评论 JSON 响应兼容的前提下，向受信任调用方额外返回内部 `parent_author_id`。Gateway 仅用它为父评论作者投递回复通知，不会将这项关系写入公开的 `CommentDto` 或评论 HTTP 响应；幂等重试会返回相同的父作者，确保通知可安全重试。

评论 HTTP/gRPC 业务入口是内部服务边界：启用 `SERVICE_AUTH_REQUIRED=true` 后，gRPC 的 `list`、`create`、`delete` 都要求 `x-service-token`，因此客户端不能绕过 Gateway 伪造用户身份或社交可见性集合；健康检查不受此限制。

## 环境变量

`COMMENT_ADDR`，默认监听 `127.0.0.1:8086`。

`CONTENT_AUDIT_GRPC_URL` 指向 `content-audit`。`STORAGE_MODE=postgres` 下未设置或审核不可用时，评论保持待审；仅无依赖的 `memory` 本地开发模式使用自动通过的本地审核器。

## 举报与申诉

`STORAGE_MODE=postgres` 已使用 SQLx/PostgreSQL 持久化评论和父子关系，并具备写入幂等、稳定游标分页和审核状态隔离。`0053_comment_moderation_queue.sql` 为待审评论建立稳定分页索引与审核决定审计记录；受信 Gateway 可列出待审队列，并以原子“通过”或“限制”决定完成审核。相同终态的重试返回原决定，不同终态的并发决定返回冲突；自动审核只能从 `reviewing` 推进，不会覆盖先完成的人审决定。

`0057_comment_report_appeal.sql` 持久化举报和申诉队列及各自按用户约束的幂等键。举报只接受发布、未删除、对举报人可见且非本人评论。举报审核的 `resolved + restrict_comment` 会在同一事务把评论从 `published` 转为 `restricted`；受限评论作者可提交申诉，`resolved + restore_comment` 会在同一事务恢复为 `published`。任一动作若不再适用则回滚整个决定，避免记录与可见性分叉。终态重复提交相同状态、说明和动作会返回原结果，其他终态覆盖尝试返回冲突。Gateway 从已验证 JWT 派生审核员 ID，并使用稳定来源键向作者投递评论审核状态通知。
