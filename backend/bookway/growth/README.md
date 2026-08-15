# Growth 成长服务

## 职责

`growth` 持有私人路线、行动计划、今日行动、行动留痕、周回望、资源知识库、陪伴简报、提醒偏好/设备注册、通知收件箱和公共路线参与意图。私人记录不会被隐式转换为公开内容。

## 接口

内部 gRPC 覆盖 Journey/Action/Entry/Review/Knowledge/Reminder/Notification 读写及 `companion` 查询；Gateway 公开今日清单和陪伴简报 `GET /v1/today?date=YYYY-MM-DD&timezone=Asia/Shanghai`、`GET /v1/companion?date=...&timezone=...`，以及完整替换式提醒偏好 `GET|PUT /v1/reminder-preferences`、设备注册 `POST /v1/push-devices`、注销 `DELETE /v1/push-devices/{device_id}` 和通知收件箱 `GET /v1/notifications`、`PATCH /v1/notifications/{notification_id}/read`。`create_route_journey` 以 `(user_id, source_route_id)` 幂等创建私人 Journey，`set_route_participation_intent` 维护加入、退出和重新加入的单调版本。

Journey 通过 `journey_type` 区分 `habit`、`project`、`quantity`、`travel` 和 `challenge`，并持有用户可读的 `completion_criteria`。创建路线时可传入有序 `stages`，首个行动用 `first_action_stage_index` 关联阶段；以后创建行动则传入阶段 ID。旧客户端和既有 JSON payload 未携带这些字段时保持兼容，类型默认为 `project`，完成标准会生成保守的类型说明。

行动的 `recurrence` 是结构化日历规则：仅支持 `daily` 和 `weekly`、正整数 `interval`、周重复的去重 `weekdays`、可选 `ends_on`，以及服务端写入的 `anchor_date`。重复行动必须同时提供带显式偏移量的 `scheduled_for` 与 IANA `scheduled_timezone`。完成或跳过一个待办 occurrence 会原子保留该次事实，并物化下一次待办；重复规则结束时不再创建后续 occurrence。客户端必须使用完成接口而非以 PATCH 直接标记 `completed`，以免丢失下一次安排。

`GET /v1/reviews/weekly` 除汇总和反思题外，还返回 `adjustment_suggestions`。每条建议包含理由及可选的 `action_patch` 或 `journey_patch` 参数，分别对应已有的 `PATCH /v1/actions/{action_id}` 和 `PATCH /v1/journeys/{journey_id}`；建议只在用户确认后由客户端应用，服务不会悄悄改变计划。

## 陪伴简报策略

`companion` 只读取当前用户进行中路线内的今日行动和回望快照，返回 `start_small`、`keep_going`、`celebrate` 或 `plan_next`。有待办时优先选择时长最短的一项；出现跳过且尚未完成行动时，建议一个更小的恢复时长。返回的 `suggested_action` 和 `suggested_minutes` 仅供客户端展示，服务不会完成、跳过、改期或缩短任何行动。

行动可同时携带展示用 `scheduled_label`、带显式 UTC 偏移量的 RFC 3339 `scheduled_for` 与 IANA `scheduled_timezone`。Growth 将本地日期写入索引字段、将精确瞬间和时区独立持久化，因此今日清单按调用方本地日期读取，陪伴简报能够识别明确安排且已经过期的待办，并仍只给出可选的小步建议。旧行动没有精确安排时间，会保留原有日期行为且不会被误判为逾期。

提醒偏好有启用状态、提前分钟数、IANA 时区和可跨午夜的 `HH:MM` 静默窗口。禁用提醒会在同一偏好更新事务中取消该用户已有的排队投递；注销设备或把全局唯一的设备 ID 绑定到另一位用户，也会取消旧设备归属的排队投递。`bookway-reminder-dispatcher` 使用 `FOR UPDATE SKIP LOCKED` 扫描已到窗口的精确安排，跳过静默时间，并在同一 SQL 语句中写入 `(action_id, schedule_revision, channel, device_id)` 去重的 `reminder_deliveries`、用户可见的通知收件箱项和 Outbox 命令。多个设备只会生成一条同一行动版本的收件箱项；改期会增加安排版本，完成、跳过或改期会取消旧的排队投递。命令不携带推送 endpoint；后续 provider consumer 必须先确认投递仍为 `queued`、行动仍为待办，才可发送。

内部 `CreateNotification` gRPC 供受信业务服务写入收件箱。`(kind, source_id)` 是全局幂等键：同一接收者重试会返回原通知，来源键若意外复用到另一位用户则拒绝写入。Gateway 在点赞、评论、关注成功后以该接口尽力创建社区通知；通知服务临时不可用只会记录降级日志，不会回滚已完成的社区操作。

## 环境变量

`GROWTH_ADDR`，默认监听 `127.0.0.1:8081`。

## 生产化待办

`STORAGE_MODE=postgres` 已提供 SQLx/PostgreSQL Repository、用户归属条件、来源路线唯一约束、参与意图事务写入、行动精确安排、提醒窗口、静默时段和去重提醒命令。下一阶段补齐行动操作幂等键、实际推送 provider consumer/endpoint 加密、通知结果回写、客户端设备令牌注册和增量同步版本。
