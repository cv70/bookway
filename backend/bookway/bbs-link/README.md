# BBS Link 内容服务

## 职责

`bbs-link` 是公开内容的唯一事实源，持有帖子元数据、正文、媒体引用、主题、作者归属、版本和审核状态。公开媒体只能通过 `media_asset_ids` 引用 Media 服务中属于作者、且已处理完成的资产；服务从受信 Media 响应生成 CDN URL 和封面，拒绝客户端提供的任意 `cover_url`。`content_type=video` 至少需要一个已完成处理的 `video/mp4` 资产；非视频内容只能引用图片，因此处理中的、越权的或媒体类型不匹配的文件都无法进入公开审核队列。

## 接口

- `POST /v1/posts`：创建内容，支持 `Idempotency-Key`。
- `GET/PATCH /v1/posts/{id}`：读取已公开内容和更新内容。
- `POST /v1/posts/{id}/publish`：发布内容，支持 `Idempotency-Key`；首次发布事务提交后，同一作者以同一键重试会返回审核完成时的内容快照，不会再次审核或递增版本。
- 内部 gRPC：`list`、`get_public_summaries`、`get`、`get_public`、`create`、`update`、`publish`、`restrict`、`restore`。启用 `SERVICE_AUTH_REQUIRED=true` 后，所有业务方法都必须携带 `x-service-token`，健康检查除外；客户端不能通过直连绕过 Gateway 的可信身份、审核和社交可见性策略。`get` 仅供受信内部的作者/审核流程使用；面向客户端及社区互动的读取必须使用 `get_public`，草稿、审核中和受限内容统一以未找到响应。`list.author_ids` 是只供受信时间流召回使用的批量作者过滤器，最多 5,000 个作者；它与单作者 `author_id` 不能混用，并按当前审核状态和全局新鲜度排序。`get_public_summaries` 供搜索和推荐在批量召回后重读当前公开事实：最多 100 个唯一内容 ID，响应只含当前已公开内容的帖子摘要、作者、类型、主题和质量分，不携带正文或媒体；缺失项必须按不可公开处理。`restrict` 是人工处置专用的幂等状态迁移；它会清除公开时间，任何重试都不会重新公开内容。`restore` 只接受此前受限的内容，避免把草稿或待审编辑绕过发布审核直接公开。

`content_type=route` 必须携带结构化 `route_template`：路线意图、完成标准、至多 12 个阶段和 1--50 个行动。每个阶段和每个行动都必须声明唯一的稳定 `id`：阶段 ID 供行动、阶段成果和问题上下文引用，行动 ID 供资源、装备和广告挂载；数组位置和标题都不能作为节点身份，因此作者重排、插入或删除阶段不会把已发布的引用静默改指到别的阶段。模板只描述可复用的方法，不包含作者的行动状态、条目、媒体、位置、精确日程或重复规则；行动的 `scheduled_label` 仅是供采用者自行安排的通用提示。非路线内容不能携带该字段。`PostSummary.is_route` 始终由 `content_type` 派生；非路线会清除 `route_title` 和 `route_duration`，因此不能把私人 Journey 名称或日程伪装成可加入的公共路线。`PostSummary.join_count` 是可选字段且**不属于内容事实**：同行人数由 BBS 的参与关系拥有，BBS Link 既不存也不自增，索引同样不存；只有在本次请求真的读到该事实的服务才会填值。字段缺失表示“未读到”，客户端不得渲染成 0 人同行。对照之下 `fork_count` 是内容表事实，由 Fork 发布事务自增。

`content_type=milestone` 是阶段成果：创建时必须提交公开 `route_id`、路线中的 `stage_id`、投入、结果、调整与证据范围。服务只接受当前已发布的路线，并从它的公开摘要和模板生成不可由客户端伪造的路线/阶段快照；领域必须相同。阶段成果不读取或保存私人 Journey、Action、Entry、精确日程、位置或身份信息，也不能携带路线模板或成为可加入路线。它和普通内容一样经过媒体门禁、审核、限制、举报/申诉和搜索 Outbox；`PostSummary.is_milestone` 始终由内容类型派生，供 Feed 和搜索结果明确展示“他人的实践证据”而非另一条可采用路线。

`content_type=question` 复用已审核的根评论作为回答，而不复制一套回答内容和审核系统。问题可选携带一条公开且同领域路线及其 `stage_id`；服务端固定 `QuestionContext` 的路线/阶段标题快照，绝不读取问题作者的私人 Journey、Action、Entry、日程或完成进度。问题作者只能通过 Gateway 的 `POST /v1/posts/{post_id}/comments/{comment_id}/accept` 采纳一条当前公开、未屏蔽、属于该问题的一级回答；Gateway 先以 BBS Link 校验问题作者和公开状态，再以 Comment 校验回答归属与可见性，最后调用内部 `accept_answer` 写入稳定的 `accepted_answer_id`。采纳不改变问题审核状态或正文，也会向回答作者投递幂等社区通知。`PostSummary.is_question` 同样由类型派生，因此搜索和 Feed 能区分问题与普通笔记。

## 环境变量

`BBS_LINK_ADDR` 和 `BBS_LINK_GRPC_ADDR`，默认分别监听 `127.0.0.1:8084`、`127.0.0.1:18004`。审核依赖使用 `CONTENT_AUDIT_GRPC_URL`，媒体归属校验依赖使用 `MEDIA_GRPC_URL`（默认 `http://127.0.0.1:18091`）。

## 生产化待办

`STORAGE_MODE=postgres` 已提供 SQLx/PostgreSQL Dao、事务幂等键、乐观版本冲突和内容审核调用。内容创建及每次状态/正文变更都会在同一事务写入 `content_index_outbox`；同一内容的多次变更合并为最新版本，避免搜索投影读取到过期审核状态。发布幂等键在同一事务保存审核完成时的响应快照；编辑已发布内容会回到 `reviewing`，但旧发布请求的延迟重试仍只返回原始结果。受限或删除内容不能由作者直接重新发布，必须经过既有的申诉/人工处置流程。媒体映射也在该事务内替换写入 `content_media`，记录确切的 Media asset、对象 key、MIME 和顺序，避免更新时遗留旧资源。Gateway 可通过内部 `list(author_id=...)` 为创作中心读取作者自己的非公开状态，但该过滤不对客户端直接开放。
