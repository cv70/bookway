# 万卷行服务等级目标

## 首阶段 SLO

| 用户旅程 | SLI | 目标（滚动 28 天） | 延迟目标 |
| --- | --- | ---: | ---: |
| Gateway API | 非 5xx 成功率 | `99.90%` | P95 `< 300ms` |
| Feed | 返回可用 Feed（允许标记降级） | `99.90%` | P99 `< 150ms` |
| Search | 返回搜索结果或显式降级 | `99.90%` | P99 `< 150ms` |
| Event ingest | 接收并持久化事件 | `99.95%` | P95 `< 100ms` |
| 内容发布 | 完成审核状态转换 | `99.90%` | P95 `< 2s` |
| 商城下单（mall-order） | 创建订单（幂等重放返回原单） | `99.95%` | P95 `< 300ms` |
| 库存预留（mall-inventory） | 预留成功或显式缺货；Redis 门闸故障降级 PG 直连不算失败 | `99.95%` | P95 `< 200ms` |
| 节点商品读（mall NodeOffer） | 按路线节点返回在售 Offer | `99.90%` | P99 `< 150ms` |
| 广告决策（ad-main decide） | 返回广告或显式空决策；上下文/频控校验失败返回空不算失败 | `99.90%` | P99 `< 150ms` |
| 广告回执（ad-center 回执） | 决策登记与曝光/点击事件持久化 | `99.95%` | P95 `< 200ms` |

异步链路目标：Outbox 最老未发布事件小于 `60s`，OpenSearch 索引延迟小于 `30s`，死信事件为 `0`，Redis 限流故障时业务采用 fail-open 并产生告警。运行时 histogram 暴露 `0.15` 秒桶，Feed/Search 的 P99 越过该桶时触发告警。

商业化护栏：跨服务调用一律经过 `bookway_runtime::grpc_channel` 的连接超时与 keep-alive 约束，热点下游叠加 `CircuitBreaker` 快速失败;Redis 频控计数只是 Eligible 预过滤,预算与频控的最终裁决始终在 PostgreSQL 事务内,两者不可互换。Feed 冷启动匿名页缓存 TTL 不超过 `5s`,个性化路径不缓存。

## 告警

- 快速燃尽：1 小时错误预算消耗率大于 `14.4x`，立即告警。
- 慢速燃尽：6 小时错误预算消耗率大于 `6x`，立即告警。
- Outbox lag 超过 `60s` 持续 5 分钟，或出现 `dead` 状态，立即告警。
- PostgreSQL 连接池等待 P95 超过 `100ms`、Redis/OpenSearch 连续失败 5 分钟，告警。

每次发布必须附带 dashboard、回滚版本和变更对应的 SLO 风险。对 Feed/Search，必须在真实 Redis、PostgreSQL 和 OpenSearch 依赖组上运行 `cargo run -p bookway-gateway-slo-loadtest` 并保留 JSON 报告。没有容量压测和故障演练数据时，不以“生产级”作为上线结论。
