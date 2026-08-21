# Growth 成长服务

## 职责

`growth` 持有私人路线、行动计划、今日行动、行动留痕、周回望、资源知识库、陪伴简报、提醒偏好/设备注册、通知收件箱和公共路线参与意图。私人记录不会被隐式转换为公开内容。

## 接口

内部 gRPC 覆盖 Journey/Action/Entry/Review/Knowledge/Reminder/Notification 读写及 `companion` 查询；Gateway 公开今日清单和陪伴简报 `GET /v1/today?date=YYYY-MM-DD&timezone=Asia/Shanghai`、`GET /v1/companion?date=...&timezone=...`，以及完整替换式提醒偏好 `GET|PUT /v1/reminder-preferences`、设备注册 `POST /v1/push-devices`、注销 `DELETE /v1/push-devices/{device_id}` 和通知收件箱 `GET /v1/notifications`、`PATCH /v1/notifications/{notification_id}/read`。`create_route_journey` 以 `(user_id, source_route_id)` 幂等创建私人 Journey，`set_route_participation_intent` 维护加入、退出和重新加入的单调版本。

普通私人 Journey 可通过 `Idempotency-Key` 重试创建：同一用户、同一键和相同的路线、阶段及首项行动只返回首次创建的 Journey；键被重用于不同初始内容则拒绝。Growth 保存规范化的首次创建快照，因此后续编辑计划、完成或调整首项行动不会把安全重试错误判为冲突。

加入公共路线时，Gateway 将公开 `route_template` 映射为 `create_route_journey` 的首项行动和 `additional_actions`；Growth 在同一事务中创建私人 Journey、阶段和至多 50 项行动。阶段索引会映射到新建的私有阶段 ID，公共模板不会保留为可变引用。相同用户重试加入只返回原有 Journey，公共路线随后编辑也绝不改写已采用的私人计划；缺少结构化模板的路线会被拒绝加入。

完成行动后，`CompleteAction` 会在响应中仅向受信调用方附带该私人 Journey 的可选来源路线 ID。此字段不进入移动端 Action JSON；Gateway 用它把已提交的采用路线行动记录成可信 `complete` 信号。普通私人路线没有来源 ID，因此不会产生任何公共内容归因。

选择“发布为行记”时，`CreateEntry` 先持久化记录与 `entry_publication_jobs`，并返回 `pending`；`entry-publication-dispatcher` 使用该条记录专属的幂等键创建 BBS 内容并提交审核。客户端可通过 `Idempotency-Key` 重试创建：同一用户、同一键和同一条目内容返回已有记录，同键不同内容会被拒绝；即使行记已经推进到审核或发布状态，重试仍不会新增记录或发布任务。可选照片仅保存 Media 资产 ID，任务再以作者身份把该 ID 交给 BBS Link 做就绪和归属校验；原始 URL 不会进入这条链路。它只带出用户填写的正文、可选照片和用于分发的粗粒度领域，绝不带出地点、心情、数量、私人路线标题等字段。审核结果会回写为 `reviewing`、`published` 或 `restricted`；连续失败进入 `failed` 后，用户可通过 `POST /v1/entries/{entry_id}/publication/retry` 显式重试。这样客户端断网、任务重启和下游超时均不会让“已发布”成为没有社区内容的假状态。

知识库资源可选携带 `source_content_id`，仅由 Gateway 将公开社区内容收集为私有引用时写入。该字段按 `(user_id, source_content_id)` 去重：同一内容反复收集或在公开内容编辑后重试都会返回既有资源，而不是制造冲突或副本。收集资源只保留展示元数据与 `bookway://content/{id}`，不复制正文或媒体；客户端打开原内容时必须重新走 BBS Link 的公开可见性与审核检查。手动资源的来源地址只接受带主机的 `http(s)` URL，服务生成的私有引用仅接受严格的 `bookway://content/{id}`，从而不会把可执行或本地 scheme 交给客户端。`StartKnowledgeJourney`（Gateway: `POST /v1/knowledge/{resource_id}/journey`）接受与创建 Journey 相同的计划和首项行动字段，在同一 Growth 事务中把资源设为 `active`、关联私有 Journey 并创建首项行动；资源行是幂等边界，并发或断网重试只会返回既有 Journey。完成这类 Journey 的行动时，若它恰好关联一条带 `source_content_id` 的资源，内部完成响应才会把原内容 ID 交给 Gateway 记录一次可信 `complete` 推荐信号；多条不同来源同时关联到同一 Journey 时不归因，避免伪造偏好。

Journey 通过 `journey_type` 区分 `habit`、`project`、`quantity`、`travel` 和 `challenge`，并持有用户可读的 `completion_criteria`。创建路线时可传入有序 `stages`，首个行动用 `first_action_stage_index` 关联阶段；以后创建行动则传入阶段 ID。当前服务对未填写的类型和完成标准执行明确的产品归一化：类型按 `project` 处理，完成标准按类型生成说明；这不是旧协议兼容分支。

行动的 `recurrence` 是结构化日历规则：仅支持 `daily` 和 `weekly`、正整数 `interval`、周重复的去重 `weekdays`、可选 `ends_on`，以及服务端写入的 `anchor_date`。重复行动必须同时提供带显式偏移量的 `scheduled_for` 与 IANA `scheduled_timezone`。完成或跳过一个待办 occurrence 会原子保留该次事实，并物化下一次待办；重复规则结束时不再创建后续 occurrence。客户端必须使用完成接口而非以 PATCH 直接标记 `completed`，以免丢失下一次安排。

`GET /v1/reviews/weekly` 除汇总和反思题外，还返回 `adjustment_suggestions`。`PUT /v1/reviews/weekly` 会保存用户的复盘结论和下周重点，并以 `(user_id, period_start, period_end)` 保留首次确认时的指标快照；同周再次保存只更新用户文字，绝不篡改历史指标。客户端确认建议时调用 `POST /v1/reviews/{review_id}/adjustments/{suggestion_index}/apply`，不能自行提交 Action/Journey patch。Growth 在一个事务中锁定复盘与目标，确认建议仍对应首次生成的待办时才更新计划并记录决定；重复确认返回原决定，目标已完成、状态或预计时长已变化时拒绝陈旧建议。

## 陪伴简报策略

`companion` 只读取当前用户进行中路线内的今日行动和回望快照，返回 `start_small`、`keep_going`、`celebrate` 或 `plan_next`。有待办时优先选择时长最短的一项；出现跳过且尚未完成行动时，建议一个更小的恢复时长。返回的 `suggested_action` 和 `suggested_minutes` 仅供客户端展示，服务不会完成、跳过、改期或缩短任何行动。

行动可同时携带展示用 `scheduled_label`、带显式 UTC 偏移量的 RFC 3339 `scheduled_for` 与 IANA `scheduled_timezone`。Growth 将本地日期写入索引字段、将精确瞬间和时区独立持久化，因此今日清单按调用方本地日期读取，陪伴简报能够识别明确安排且已经过期的待办，并仍只给出可选的小步建议。未提供精确安排时间的行动会保留其本地日期行为且不会被误判为逾期。

提醒偏好有启用状态、提前分钟数、IANA 时区和可跨午夜的 `HH:MM` 静默窗口。禁用提醒会在同一偏好更新事务中取消该用户已有的排队投递；注销设备或把全局唯一的设备 ID 绑定到另一位用户，也会取消旧设备归属的排队投递。`bookway-reminder-dispatcher` 使用 `FOR UPDATE SKIP LOCKED` 扫描已到窗口的精确安排，跳过静默时间，并在同一 SQL 语句中写入 `(action_id, schedule_revision, channel, device_id)` 去重的 `reminder_deliveries`、用户可见的通知收件箱项和 Outbox 命令。多个设备只会生成一条同一行动版本的收件箱项；改期会增加安排版本，完成、跳过或改期会取消旧的投递。`bookway-push-delivery-dispatcher` 用持久租约领取投递，按行动、设备和投递的锁顺序复核当前版本再发送；它向受信 `PUSH_DELIVERY_GATEWAY_URL` 传递稳定 `delivery_id` 幂等键，Gateway 必须返回 `sent`、`duplicate`、`invalid_device` 或带 `retryable` 的失败。超时会退避重试，永久失败留存为 `failed`，失效设备会被撤销，且任何一次发送都不会把 endpoint 写进 Outbox 或日志。

内部 `CreateNotification` gRPC 供受信业务服务写入收件箱。`(kind, source_id)` 是全局幂等键：同一接收者重试会返回原通知，来源键若意外复用到另一位用户则拒绝写入。Gateway 在点赞、评论、关注成功后以该接口尽力创建社区通知；通知服务临时不可用只会记录降级日志，不会回滚已完成的社区操作。

## 环境变量

`GROWTH_ADDR`，默认监听 `127.0.0.1:8081`。

## 生产化待办

`STORAGE_MODE=postgres` 已提供 SQLx/PostgreSQL Dao、用户归属条件、来源路线唯一约束、参与意图事务写入、行动精确安排、提醒窗口、静默时段、去重提醒命令、Provider 投递 Worker 和记录到行记的可恢复发布任务。客户端以 `Idempotency-Key` 创建行动时，同一用户、同一键和同一动作内容会返回已有行动；键被复用于不同内容则拒绝，避免弱网重试或重复点击生成第二条待办。下一阶段补齐设备 endpoint 静态加密、Provider 回执明细、客户端设备令牌注册和增量同步版本。
