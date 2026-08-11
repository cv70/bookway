# Comment 评论服务

## 职责

`comment` 持有评论正文、父子关系、审核状态和评论列表。帖子是否存在由 Gateway 在写入前通过 `bbs-link` 校验。

## 接口

- `GET /v1/posts/{post_id}/comments`
- `POST /v1/posts/{post_id}/comments`

服务拒绝空评论、超长评论，以及不属于同一帖子的父评论。

## 环境变量

`COMMENT_ADDR`，默认监听 `127.0.0.1:8086`。

## 生产化待办

`STORAGE_MODE=postgres` 已使用 SQLx/PostgreSQL 持久化评论和父子关系。下一阶段增加稳定游标分页、审核队列、评论计数、回复深度限制、重复/垃圾检测、举报申诉和异步搜索/推荐事件。
