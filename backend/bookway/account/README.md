# Account 账户服务

## 职责

`account` 持有用户可编辑的公开资料：昵称、头像 URL 和个人简介。它不保存密码、验证码、OAuth token 或 JWT；身份认证始终由 Gateway 验证 Bearer JWT 后，以受服务令牌保护的内部 gRPC 传入 `user_id`。

资料使用“首次读取时创建”的方式初始化。这样不会把身份系统、资料系统和社区关系数据混到同一个服务中。

## 内部契约

- `Profile(user_id)`：读取或创建当前用户资料。
- `UpdateProfile(user_id, request)`：更新当前用户资料；昵称为 1-40 个字符，简介最多 160 个字符，头像只能是 `http(s)` URL 或空字符串（清空）。

Gateway 将它们暴露为：

- `GET /v1/me/profile`
- `PATCH /v1/me/profile`

启用 `SERVICE_AUTH_REQUIRED=true` 时，账户 RPC 需要 `x-service-token`，因此不能绕过 Gateway 冒用 `user_id`。

## 数据与运行

`0030_account.sql` 创建 `account_profiles` 表；在 PostgreSQL 模式下先执行 `cargo run -p bookway-db-migrate`。`ACCOUNT_ADDR` 默认是 `127.0.0.1:8094`，Gateway 使用 `ACCOUNT_GRPC_URL` 连接它。
