# BBS Creator 创作者档案服务

`bbs-creator` 持有创作者经营页的独有事实：唯一 handle、行动方法定位、专长、精选内容、封面和私信接收意愿。`account` 仍是用户显示名和头像的唯一来源，`bbs-link` 仍是内容事实来源；Gateway 在读路径中聚合这些契约，避免复制账户或内容数据。

## 接口

- `GetProfile`：读取一位创作者的公开经营档案。
- `UpsertProfile`：创建或更新自己的档案；handle 以大小写无关的方式全局唯一。
- `ListProfiles`：按用户 ID、关键词或专长发现创作者，使用 `(updated_at, user_id)` 稳定游标；发现流默认只返回 `active` 档案，明确按 ID 读取时保留暂停档案。

`bbs-message` 单独持有私信接收意愿；创作者档案不复制私信偏好，避免两个服务对同一隐私开关形成双写源。

Gateway 会在搜索的用户结果上批量聚合 `active` 档案，让“找人”不仅停留在历史内容作者，也能展示该创作者擅长帮助用户完成什么。暂停档案不会作为搜索用户结果返回；基础内容搜索由 `search-main` 继续负责，创作者服务不可用只使扩展字段降级。

## 运行

HTTP 默认监听 `127.0.0.1:8105`，gRPC 默认监听 `127.0.0.1:18105`。可通过 `BBS_CREATOR_ADDR` 和 `BBS_CREATOR_GRPC_ADDR` 修改。`STORAGE_MODE=memory` 适合本地开发；生产使用 PostgreSQL，并先执行 `0054_bbs_creator_message.sql`。
