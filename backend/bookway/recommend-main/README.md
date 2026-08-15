# Recommend Main 推荐主服务

## 职责

在线推荐引擎，负责候选编排、补全、过滤、打分、多样性重排和曝光事件，不持有内容或社交事实数据。

## 流水线

```text
Query Hydration
-> 多路内容召回
-> BBS 社交图谱补全
-> 点赞/收藏状态补全
-> 安全与客户端已看过滤
-> 质量/意图/多样性打分
-> Selector（优先未曝光、受控回补）
-> 持久化曝光
```

候选召回由 `recommend-recall` 提供，最终模型排序由 `recommend-rank` 提供；首页召回前会在 150ms 预算内读取 `feature-main` 的 `domain_interest.*`，将真实行为偏好的领域并入客户端兴趣，特征暂不可用时保留原始兴趣并标记降级。`surface=following` 则先从受信 BBS 图谱取得关注作者集合，并将其作为 Recall 的批量作者约束：空集合返回空时间流，绝不回退全局候选；该页面保留 BBS Link 的稳定最新顺序，不使用兴趣扩展、模型排序、服务历史回补或多样性打散。翻页 cursor 绑定规范化关注作者集合，关系变化时会从当前集合的第一页重新开始，避免将旧 offset 应用于新的社交时间窗。过滤、打分和多样性属于本服务内的编排阶段，不单独拆成网络服务。社交补全同时读取用户关系和受服务令牌保护的可见性策略，因此当前用户主动拉黑/静音的作者以及拉黑当前用户的作者都会在安全过滤前标记并剔除。

曝光会在 Feed 响应返回前同时写入 `feed_exposures` 和 `feed_exposure_items`，以及实际的 `model_version` 和 `experiment_bucket`。近期服务历史在 Selector 中优先让位给未曝光内容，但不是硬过滤：当未曝光候选不足时才受控回补旧曝光，并显式附带原因，避免小候选池出现空 Feed；客户端明确提交的 `seen`、隐藏、拉黑和静音仍是硬过滤。内部 `ValidateAttributions` gRPC 契约按批次核对可信用户、会话、`request_id`、内容和排序位置，让 User Event 只保留真实曝光产生的训练归因；持久化失败会将该 Feed 标记为降级，后续事件会安全地退化为无归因反馈。

## 依赖与环境变量

- 依赖：`bbs`、`commonlikestatus`、`feature-main`、`recommend-recall`、`recommend-rank`。
- `RECOMMEND_MAIN_ADDR`：默认 `127.0.0.1:8083`。
- `BBS_GRPC_URL`、`LIKE_STATUS_GRPC_URL`、`FEATURE_MAIN_GRPC_URL`、`RECOMMEND_RECALL_GRPC_URL`、`RECOMMEND_RANK_GRPC_URL`：上游 gRPC 服务地址。BBS 调用在 `SERVICE_AUTH_REQUIRED=true` 下携带 `x-service-token`。

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
