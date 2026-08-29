# Recommend Main 推荐主服务

## 职责

在线推荐引擎，负责候选编排、补全、过滤、打分、多样性重排、场景广告混排和曝光事件，不持有内容或社交事实数据。

## 流水线

```text
Query Hydration
-> 多路内容召回
-> BBS 社交图谱补全
-> 点赞/收藏状态补全
-> 安全与客户端已看过滤
-> 质量/意图/多样性打分
-> 粗排（保留召回来源与领域覆盖）
-> 多目标精排（pCTR / pCVR / pWEGU / route_completion_rate）
-> Selector（频控优先、作者/领域打散、受控回补）
-> Action Node 场景广告 eCPM 混排
-> 持久化曝光
```

历史、频控、社交、路线和反应事实水合彼此独立时并行发起；每个水合器只合并自己拥有的字段，避免并发快照互相覆盖。依赖这些事实的社交证明水合器最后串行运行，因此并行化不会改变可解释推荐理由或安全 fail-closed 语义。

候选召回由 `recommend-recall` 提供，最终模型排序由 `recommend-rank` 提供；首页召回前会在 150ms 预算内读取 `feature-main` 的 `domain_interest.*`，将真实行为偏好的领域并入客户端兴趣，特征暂不可用时保留原始兴趣并标记降级。精排同时计算校准后的 pCTR、pCVR、行动完成概率 pWEGU 和整条公开路线的 `route_completion_rate`，WEGU 与路线完成度权重高于点击和购买代理信号；缺少分母时回退到有限的冷启动先验，禁止 NaN/无穷值进入排序。`surface=following` 则先从受信 BBS 图谱取得关注作者集合，并将其作为 Recall 的批量作者约束：空集合返回空时间流，绝不回退全局候选；该页面保留 BBS Link 的稳定最新顺序，不使用兴趣扩展、模型排序、服务历史回补或多样性打散。翻页 cursor 绑定规范化关注作者集合，关系变化时会从当前集合的第一页重新开始，避免将旧 offset 应用于新的社交时间窗。过滤、打分和多样性属于本服务内的编排阶段，不单独拆成网络服务。社交补全同时读取用户关系和受服务令牌保护的可见性策略，因此当前用户主动拉黑/静音的作者以及拉黑当前用户的作者都会在安全过滤前标记并剔除。

只有 Feed 请求同时携带已认证用户、`route_id` 和 `action_node_id` 时，服务才会向 `ad-main` 请求决策；匿名请求即使带有路线节点上下文也保持纯有机。`ad-main` 完成定向、eCPM 竞价和频控后，推荐主服务再次校验广告的路线、节点和广告位完全匹配，然后按 `pkg/commercial-mix` 的密度调度混入有机列表：默认负载为整页的 ~10%，页首三位始终保持纯自然结果，槽位深度由共享调度的比例表给出（首槽位于页面四分之一深度之后）。曝光在最终混排之后持久化；被广告挤出的尾部自然结果不会进入曝光账本或频控计数，因 Feed 的不透明召回游标无法回退而仅在本页跳过。决策请求的数量即调度允许的槽数，供给永不超出页面可合法渲染的商业位。缺失场景、上游不可用或返回不匹配广告时，一律返回纯有机 Feed，并标记 `meta.degraded=true`（仅上游故障时）。

曝光会在 Feed 响应返回前同时写入 `feed_exposures` 和 `feed_exposure_items`，以及实际的 `model_version` 和 `experiment_bucket`。曝光条目的 position 保留最终 FeedItem 的视觉位置（包括广告插卡），因此客户端上报的位置可以精确命中账本；广告本身没有有机曝光条目。近期服务历史在 Selector 中优先让位给未曝光内容，但不是硬过滤：当未曝光候选不足时才受控回补旧曝光，并显式附带原因，避免小候选池出现空 Feed；客户端明确提交的 `seen`、隐藏、拉黑和静音仍是硬过滤。内部 `ValidateAttributions` gRPC 契约按批次核对可信用户、会话、`request_id`、内容和排序位置，让 User Event 只保留真实曝光产生的训练归因；持久化失败会将该 Feed 标记为降级，后续事件会安全地退化为无归因反馈。

## 服务端硬曝光频控

在温和的历史疲劳信号之上另有每日硬上限：同一内容对同一用户当日达到 `FEED_FREQUENCY_CAP_DAILY`（默认 3）次后，将从后续 Feed 中直接剔除。计数键为 `fcap:{user}:{content}:{yyyymmdd}`，写入发生在曝光持久化之后（单次 Lua 原子批量 INCR，当日首增安装 48 小时 TTL）；读取在精排前一次 MGET 批量完成。Redis 故障时读侧整体放行并标记降级、写侧仅告警——护栏按 fail-open 设计，绝不因计数器故障导致空 Feed 或请求失败；Postgres 模式下未配置 `REDIS_URL` 时护栏显式关闭并在启动时告警。

## 依赖与环境变量

- 依赖：`bbs`、`interaction-status`、`feature-main`、`recommend-recall`、`recommend-rank`、`ad-main`。
- `RECOMMEND_MAIN_ADDR`：默认 `127.0.0.1:8083`。
- `BBS_GRPC_URL`、`INTERACTION_STATUS_GRPC_URL`、`FEATURE_MAIN_GRPC_URL`、`RECOMMEND_RECALL_GRPC_URL`、`RECOMMEND_RANK_GRPC_URL`、`AD_MAIN_GRPC_URL`：上游 gRPC 服务地址。BBS 与广告调用在 `SERVICE_AUTH_REQUIRED=true` 下携带 `x-service-token`。
- `FEED_FREQUENCY_CAP_DAILY`：同一内容对同一用户的每日硬曝光上限，默认 3，设为 0 关闭护栏（Postgres 模式还需 `REDIS_URL`）。
- `FEED_ANON_PAGE_TTL_SECS`：冷启动匿名首页共享快照 TTL，默认 3 秒，设为 0 关闭页缓存（Postgres 模式还需 `REDIS_URL`）。

## 冷启动首页页缓存

无用户标识、无翻页 cursor、无已看列表、无声明兴趣且无 Action Node 上下文的首页请求——其自然排序在一个极短窗口内对所有调用者完全一致——会以 `surface|limit` 为键共享同一份页面快照（Redis + `pkg/cache` 读穿缓存，进程内与跨实例 miss 单飞），默认 TTL 仅 3 秒：只用于吸收发布或推广瞬间的召回风暴，不是个性化缓存。广告混排在缓存之外按请求独立进行，因此商业化频控和 eCPM 竞价不受影响。任何个性化输入（登录用户、声明兴趣、已看列表、上下文节点）都完全绕过该缓存；无 Redis 时功能等价于直接执行流水线。

`STORAGE_MODE=postgres` 时曝光及曝光条目持久化到 PostgreSQL。远程特征、模型或曝光持久化不可用时保留流水线启发式得分，并返回 `meta.degraded=true`；但社交可见性（拉黑/静音/被拉黑）或用户隐藏状态的补全失败时，未知绝不视为允许，服务会返回空的降级 Feed 并清除翻页游标，等待下一次请求安全重试。`ValidateAttributions` 与 Feed 都是内部 gRPC，在 `SERVICE_AUTH_REQUIRED=true` 下要求服务令牌。

## 离线回放评估

`job/recommendation-evaluator` 是一次性、只读事实表的评估任务，须在迁移后以 PostgreSQL 运行：

```bash
RECOMMEND_EVAL_START_AT=2026-08-01T00:00:00Z \
RECOMMEND_EVAL_CUTOFF_AT=2026-08-08T00:00:00Z \
cargo run -p bookway-recommendation-evaluator
```

评估只读取未降级的 `feed_exposures` / `feed_exposure_items`，并只关联 User Event 在服务端 `received_at` 落于该曝光标签窗口（默认 168 小时）内的、已核验归因事件。任务会排除截止时间前不足一个标签窗口的曝光，输出并持久化无用户标识的评估快照；按 `surface`、流水线、模型版本和实验桶报告渲染率、点击/查看/高意图/负反馈率、净效用、可见内容的 NDCG@5 与 Top-3 效用捕获率。`RECOMMEND_EVAL_MIN_RENDERED_ITEMS` 默认 1000，样本不足时快照明确标为 `insufficient_data`，不能作为发布判断。

为可重复比较，应显式固定 `RECOMMEND_EVAL_START_AT`、`RECOMMEND_EVAL_CUTOFF_AT` 和 `RECOMMEND_EVAL_LABEL_WINDOW_HOURS`；任务会保留每次解析后的窗口与指标。该任务衡量已服务列表上的观察性反馈，不提供反事实因果结论，也不会自动训练、选中或发布模型。

## 生产化待办

增加独立召回索引、训练和特征管理、在线实验配置、反事实评估、热点保护、模型漂移监控和一键回滚。Feed 产品编排已经由 `bbs-feed` 独立承接，推荐主服务保持在线决策边界。
