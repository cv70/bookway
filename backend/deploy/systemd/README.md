# systemd 部署单元

万卷行后端以 Cargo workspace 构建、以 systemd 承载：长驻服务用 `.service`
（`Restart=always`），一次性维护作业用 `.service + .timer` 组合（`Persistent=true`
保证错过的调度在恢复后补跑一次）。

| 单元 | 形态 | 节奏 | 产物 |
| --- | --- | --- | --- |
| `bookway-model-serving` | 长驻 | 常驻 | MiniCPM5-1B 推理：OpenAI 兼容 embeddings + LLM 打分（checkpoint 门控） |
| `bookway-rank-model-trainer` | oneshot | 每周二 03:30 | PyTorch 训练产物（logistic artifact） |
| `bookway-rank-llm-trainer` | oneshot | 手动/GPU 训练机 | MiniCPM LoRA checkpoint；holdout 门控通过后原子发布注册表供 model_serving 热加载 |
| --- | --- | --- | --- |
| `bookway-search-indexer` | 长驻 | 常驻 | content_index_outbox -> OpenSearch 投影 |
| `bookway-search-index-outbox-recovery` | oneshot | 每日 05:10 | 死信审计报告（重排死信是具名人工操作，不在此单元内） |
| `bookway-search-index-reconcile` | oneshot | 每日 04:30 | 索引对账结论（完整跑完且 Outbox 清空才可为 healthy） |
| `bookway-search-evaluator` | oneshot | 每周一 06:00 | 搜索质量观察快照（不自动激活改写版本） |
| `bookway-recommendation-evaluator` | oneshot | 每周一 06:30 | 推荐质量观察快照（只用于已验证、未降级曝光） |
| --- | --- | --- | --- |
| `bookway-outbox-relay` | 长驻 | 常驻 | 事务 outbox -> 事件总线投递（下游事实链的源头） |
| `bookway-reminder-dispatcher` | 长驻 | 常驻 | 行程/复盘提醒派发 |
| `bookway-appeal-notification-dispatcher` | 长驻 | 常驻 | 申诉解禁通知派发 |
| `bookway-content-report-restriction-dispatcher` | 长驻 | 常驻 | 内容举报处罚到期解除 |
| `bookway-mall-order-expirer` | 长驻 | 常驻 | 订单过期关闭 + 分账冷静期晋级（PromoteAffiliateSettlements） |
| `bookway-mall-inventory-sweeper` | 长驻 | 常驻 | 库存预留 TTL 回收（防库存泄漏） |
| `bookway-route-participation-reconciler` | 长驻 | 常驻 | 路线参与事实对账（join/quit 漂移修复） |
| `bookway-knowledge-embedding-builder` | 长驻 | 常驻 | RAG 节点资源向量构建（0075 builder 状态机） |

`cmd/db-migrate` 是部署流水线工具（迁移执行器），不设 systemd 单元，
由发布流程在滚动更新前调用。

安装约定：

1. `cargo build --release` 产物部署到 `/opt/bookway/backend/target/release/`。
2. 环境变量统一放 `/etc/bookway/backend.env`（600 权限），单元按需引用。
3. `cp deploy/systemd/* /etc/systemd/system/ && systemctl daemon-reload`，
   长驻服务 `systemctl enable --now bookway-search-indexer bookway-outbox-relay bookway-mall-order-expirer ...`，
   计时器 `systemctl enable --now bookway-search-*.timer bookway-*-evaluator.timer`。
4. 对账/评估失败必须进入告警（`systemctl list-timers --failed` 或/job 非零退出码）；
   这些作业是发布前完整性证据的来源，静默失败等于没有证据。
