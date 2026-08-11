# Gateway 网关服务

## 职责

`gateway` 是移动端唯一访问入口，负责对外 API 版本、CORS、请求聚合以及上游传输错误和领域错误的统一转换。该服务不持有业务数据。

## 对外接口

- `GET /v1/feed`：推荐流。
- `GET /v1/search`、`GET /v1/search/suggestions`：搜索与联想词。
- `POST /v1/events`：批量上报曝光、点击等用户行为。
- `POST /v1/media/upload-url`、`POST /v1/media/{id}/complete`：对象存储直传控制面。
- `GET /v1/media/{id}`：媒体元数据与 CDN 地址。
- `/v1/journeys`、`/v1/today`、`/v1/actions/*`：路线与行动。
- `/v1/posts/*`：内容、评论和互动。
- `/v1/users/*`：关注、拉黑和静音关系。

点赞和评论写入前，Gateway 会先通过 `bbs-link` 校验内容存在且已公开。事件上报由 Gateway 注入可信用户身份后批量转发给 `user-event`。`AUTH_REQUIRED=true` 时 Gateway 校验 HS256 Bearer JWT 并以 `sub` 注入 `x-user-id`；下游调用使用 `x-service-token`，不会把客户端身份头当作可信来源。关闭鉴权仅用于本地内存模式。

## 依赖

`growth`、`bbs-feed`、`search-main`、`user-event`、`bbs-link`、`bbs`、`comment`、`commonlikestatus`、`media`。

## 环境变量

`GATEWAY_ADDR`、`GROWTH_URL`、`BBS_FEED_URL`、`BBS_LINK_URL`、`SEARCH_MAIN_URL`、`USER_EVENT_URL`、`BBS_URL`、`COMMENT_URL`、`LIKE_STATUS_URL`、`MEDIA_URL`、`AUTH_REQUIRED`、`AUTH_JWT_SECRET`、`SERVICE_AUTH_TOKEN`、`HTTP_CONNECT_TIMEOUT_MS`、`HTTP_REQUEST_TIMEOUT_MS`、`REDIS_URL`、`REDIS_CONNECT_TIMEOUT_MS`、`REDIS_COMMAND_TIMEOUT_MS`、`RATE_LIMIT_PER_MINUTE`。

## 生产化待办

当前已接入 JWT、服务令牌、请求 ID、Redis 限流和统一调用超时。下一阶段补齐 OIDC/JWKS 与密钥轮换、接口级限流策略、熔断、OpenTelemetry 上下文传播、OpenAPI 契约和分接口容量压测。
