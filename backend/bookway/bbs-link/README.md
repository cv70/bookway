# BBS Link 内容服务

## 职责

`bbs-link` 是公开内容的唯一事实源，持有帖子元数据、正文、媒体引用、主题、作者归属、版本和审核状态。公开媒体只能通过 `media_asset_ids` 引用 Media 服务中属于作者、且已处理完成的资产；服务从受信 Media 响应生成 CDN URL 和封面，拒绝客户端提供的任意 `cover_url`。`content_type=video` 至少需要一个已完成处理的 `video/mp4` 资产；非视频内容只能引用图片，因此处理中的、越权的或媒体类型不匹配的文件都无法进入公开审核队列。

## 接口

- `POST /v1/posts`：创建内容，支持 `Idempotency-Key`。
- `GET/PATCH /v1/posts/{id}`：读取已公开内容和更新内容。
- `POST /v1/posts/{id}/publish`：发布内容，支持 `Idempotency-Key`；首次发布事务提交后，同一作者以同一键重试会返回审核完成时的内容快照，不会再次审核或递增版本。
- 内部 gRPC：`list`、`get_public_summaries`、`get`、`get_public`、`create`、`update`、`publish`、`restrict`、`restore`。启用 `SERVICE_AUTH_REQUIRED=true` 后，所有业务方法都必须携带 `x-service-token`，健康检查除外；客户端不能通过直连绕过 Gateway 的可信身份、审核和社交可见性策略。`get` 仅供受信内部的作者/审核流程使用；面向客户端及社区互动的读取必须使用 `get_public`，草稿、审核中和受限内容统一以未找到响应。`list.author_ids` 是只供受信时间流召回使用的批量作者过滤器，最多 5,000 个作者；它与单作者 `author_id` 不能混用，并按当前审核状态和全局新鲜度排序。`get_public_summaries` 供搜索和推荐在批量召回后重读当前公开事实：最多 100 个唯一内容 ID，响应只含当前已公开内容的帖子摘要、作者、类型、主题和质量分，不携带正文或媒体；缺失项必须按不可公开处理。`restrict` 是人工处置专用的幂等状态迁移；它会清除公开时间，任何重试都不会重新公开内容。`restore` 只接受此前受限的内容，避免把草稿或待审编辑绕过发布审核直接公开。

`content_type=route` 可携带结构化 `route_template`：路线意图、完成标准、至多 12 个阶段和 1--50 个行动。模板只描述可复用的方法，不包含作者的行动状态、条目、媒体、位置、精确日程或重复规则；行动的 `scheduled_label` 仅是供采用者自行安排的通用提示。非路线内容不能携带该字段。`PostSummary.is_route` 始终由 `content_type` 派生；非路线会清除 `route_title` 和 `route_duration`，因此不能把私人 Journey 名称或日程伪装成可加入的公共路线。为兼容未升级客户端，未提供模板的新增路线会获得受限的一项默认行动；历史路线仍可读取，并由 Gateway 在加入时走同样的安全回退。

## 环境变量

`BBS_LINK_ADDR` 和 `BBS_LINK_GRPC_ADDR`，默认分别监听 `127.0.0.1:8084`、`127.0.0.1:18004`。审核依赖使用 `CONTENT_AUDIT_GRPC_URL`，媒体归属校验依赖使用 `MEDIA_GRPC_URL`（默认 `http://127.0.0.1:18091`）。

## 生产化待办

`STORAGE_MODE=postgres` 已提供 SQLx/PostgreSQL Repository、事务幂等键、乐观版本冲突和内容审核调用。内容创建及每次状态/正文变更都会在同一事务写入 `content_index_outbox`；同一内容的多次变更合并为最新版本，避免搜索投影读取到过期审核状态。发布幂等键在同一事务保存审核完成时的响应快照；编辑已发布内容会回到 `reviewing`，但旧发布请求的延迟重试仍只返回原始结果。受限或删除内容不能由作者直接重新发布，必须经过既有的申诉/人工处置流程。媒体映射也在该事务内替换写入 `content_media`，记录确切的 Media asset、对象 key、MIME 和顺序，避免更新时遗留旧资源。Gateway 可通过内部 `list(author_id=...)` 为创作中心读取作者自己的非公开状态，但该过滤不对客户端直接开放。
