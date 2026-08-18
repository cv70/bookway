# BBS Message 私信服务

`bbs-message` 持有一对一会话、私信正文、已读状态、私信偏好、私信举报与发送限制。它不复制社交关系：发信时 `domain` 直接使用 `BbsClient` 读取双方的 block 边。双方任一方拉黑，或接收方关闭私信，均无法发送。生产模式中，消息和私信通知 Outbox 在同一事务中提交，独立 Worker 再将无正文的导航通知可靠投递至 Growth 收件箱。

## 接口

- `Send`：只支持文本消息；要求稳定的 `client_message_id`，弱网重试返回同一消息。写入前先由 `content-audit` 审核：通过才持久化，待审、受限或审核不可用都不会进入会话。PostgreSQL 事务还会以消息 ID 写入幂等通知任务，因此 Growth 或推送链路短暂不可用不会影响私信送达和后续通知重放。
- `ListConversations`：按最后消息时间倒序稳定分页，并返回对当前用户的未读数。
- `ListMessages`：从最新消息向更早消息稳定翻页，但每一页内按时间正序返回，适合直接渲染聊天窗口。
- `MarkConversationRead`：仅将当前用户收到的消息标为已读。
- `GetPreferences` / `UpdatePreferences`：读取或设置本人是否接受私信。
- `Report`：只有原消息接收者可举报收到的私信；`Idempotency-Key` 在举报人范围内去重，复用到不同消息、原因或说明会冲突。
- `ListReports` / `ReviewReport`：仅供受服务令牌保护的内部审核调用。队列以 `(created_at, id)` 稳定续页，返回原消息正文只用于可信审核上下文。审核员可标记 `reviewing`，或给出带说明的 `resolved` / `rejected` 终态决定；`resolved + restrict_sender` 会持久化限制并阻止该发送者之后的新私信写入。

Gateway 暴露会话、消息、已读和偏好 REST 端点；移动端个人页提供会话中心，创作者主页提供发起私信入口，接收方可在会话中直接举报消息。客户端发送和举报都携带幂等键，审核拒绝和接收方关闭私信会显示可重试的安全提示，而不会伪造已送达状态。

## 运行

HTTP 默认监听 `127.0.0.1:8106`，gRPC 默认监听 `127.0.0.1:18106`。`BBS_GRPC_URL` 默认为 `http://127.0.0.1:18002`；`CONTENT_AUDIT_GRPC_URL` 指向审核服务。生产的 `STORAGE_MODE=postgres` 必须配置后者，服务会在启动时拒绝无审核依赖的配置，运行中审核不可用同样 fail-closed。`memory` 模式仅用于无依赖本地开发，明确使用本地自动通过审核器。执行 `0054_bbs_creator_message.sql`、`0055_bbs_message_moderation.sql` 和 `0056_bbs_message_notification_delivery.sql` 后，启动服务与 `bookway-direct-message-notification-dispatcher`。
