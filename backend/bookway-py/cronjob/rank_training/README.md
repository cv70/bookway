# rank_training — 推荐排序模型训练闭环（cronjob）

对齐 `x-algorithm` 的按域组织方式：训练归训练侧（本目录，Python + PyTorch），
服务归服务侧（`bookway/recommend-rank` 的 `LinearPredictor` 消费 logistic 产物；
`bg/model_serving` 消费 LoRA checkpoint 提供 LLM 打分）。

## 闭环

```text
曝光账本(feed_exposure_items.feature_snapshot, 迁移 0085)
  → train.py（logistic 三头 + 时间切分 holdout）→ artifact JSON
      → recommend-rank 以 RECOMMEND_RANK_MODEL_ARTIFACT 加载
  → train_llm.py（MiniCPM5-1B LoRA 打分头）→ checkpoint 目录 + 注册表发布
      → bg/model_serving 热加载（MODEL_REGISTRY_PATH），/score 就绪
  → 新一轮曝光（模型版本号随账本回流）
```

## 自动发布（train_llm.py → model_serving）

`train_llm.py` 产物目录含两半契约：`adapter/`（LoRA）+ `scoring_head.pt`。
发布受 holdout 门控：每个可评估目标的 AUC 必须 ≥ `TRAINER_LLM_MIN_AUC`
（默认 0.55），否则 exit 非零、注册表保持指向旧模型（checkpoint 目录仍保留
供离线检查）。门控通过后以 原子写（tmp + rename）发布 `TRAINER_REGISTRY_PATH`；
model_serving 以 `MODEL_REGISTRY_PATH` 指向同一文件，`/score` 每次调用先
stat 注册表、变更即热加载，无需重启。注册表未设置时只产 checkpoint、不发布。

## 运行

```bash
pip install -r requirements.txt
DATABASE_URL=postgres://... \
TRAINER_OUTPUT_PATH=/opt/bookway/models/rank-model-artifact.json \
python train.py
```

| 环境变量 | 默认 | 说明 |
| --- | --- | --- |
| `DATABASE_URL` | 必填 | 只读副本连接串 |
| `TRAINER_OUTPUT_PATH` | `rank-model-artifact.json` | artifact 输出；旁边另写 `.report.json` |
| `TRAINER_LABEL_WINDOW_DAYS` | 28 | 取多少天内的曝光做训练集 |
| `TRAINER_ATTRIBUTION_WINDOW_DAYS` | 7 | 曝光后多少天内的事件计为标签 |
| `TRAINER_MIN_POSITIVES` | 200 | 每个头正样本下限，不足则跳过该头 |
| `TRAINER_EPOCHS` / `TRAINER_LEARNING_RATE` / `TRAINER_L2` | 400 / 0.08 / 1e-4 | logistic 拟合超参 |
| `TRAINER_HOLDOUT_FRACTION` | 0.2 | 时间切分的 holdout 占比 |

护栏（诚实边界）：样本总量 < 2×下限拒绝训练；ctr 头正样本不足拒绝出产物
（融合公式会失去下限）；不足的头以先验 bias + 零权重导出——即"这个目标
诚实地说还没有学到东西"。上线由人工/发布流程将产物指向消费方（坏文件会
拒绝加载，版本号进曝光账本可回滚），不做自动发布。

LLM 路线的发布同样受门控但**注册表可自动热加载**（见上节）：护栏在发布前，
不在服务侧。

| 环境变量（train_llm.py） | 默认 | 说明 |
| --- | --- | --- |
| `TRAINER_LLM_OUTPUT_DIR` | `minicpm-ranker-output` | checkpoint 产物目录（adapter/ + scoring_head.pt + training_meta.json） |
| `TRAINER_REGISTRY_PATH` | 未设置 | 注册表 JSON 发布路径（与 model_serving 的 `MODEL_REGISTRY_PATH` 一致）；未设置则只产 checkpoint |
| `TRAINER_LLM_MIN_AUC` | 0.55 | 发布门控：每个可评估 holdout 目标 AUC 下限 |
| `TRAINER_LLM_EPOCHS` / `TRAINER_LLM_LEARNING_RATE` | 2 / 1e-4 | LoRA 微调超参 |
| `TRAINER_LORA_R` / `TRAINER_LORA_ALPHA` | 8 / 16 | LoRA 秩与缩放（target q_proj/v_proj） |
| `TRAINER_LLM_BATCH_SIZE` | 8 | 批大小（GPU 显存相关） |
