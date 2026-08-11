# Growth 成长服务

## 职责

`growth` 持有私人路线、行动计划、今日行动和完成状态。私人记录不会被隐式转换为公开内容。

## 内部接口

- `GET /internal/v1/journeys`
- `POST /internal/v1/journeys`
- `GET /internal/v1/today`
- `POST /internal/v1/actions/{action_id}/complete`

## 环境变量

`GROWTH_ADDR`，默认监听 `127.0.0.1:8081`。

## 生产化待办

`STORAGE_MODE=postgres` 已提供 SQLx/PostgreSQL Repository 和用户归属条件。下一阶段补齐行动操作幂等键、时区调度、周期回望、客户端增量同步版本和领域事件 Outbox。
