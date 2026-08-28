# bookway-py — Python 侧作业与服务

Rust 工作区承载在线业务服务；本目录承载**全部 Python 工作负载**，按运行形态
分三类（与 Rust 侧 `job/`、`cronjob/`、`bg/` 的命名约定一致）：

| 子目录 | 形态 | 调度方式 | 现有内容 |
| --- | --- | --- | --- |
| `cronjob/` | 定时作业 | systemd timer（`deploy/systemd/*.timer`） | `rank_training/`：排序模型训练（logistic artifact + MiniCPM LoRA） |
| `bg/` | 常驻服务 | systemd service（`Restart=always`） | `model_serving/`：MiniCPM5-1B 推理（embeddings + LLM 打分） |
| `job/` | 一次性作业 | 人工/流水线触发 | 暂无 |

约定：

1. 每个作业/服务一个子目录，自带 `requirements.txt` 与 `README.md`；
   依赖装进独立 venv，不与业务服务共享环境。
2. 作业必须遵守诚实护栏：样本/护栏不满足时**拒绝产出**（exit 非零），
   不写"看起来可用"的占位产物。
3. 定时作业由 `deploy/systemd/` 的 timer 驱动，`Persistent=true` 补跑错过的调度；
   失败必须进告警。
4. 训练产物契约（artifact JSON / checkpoint 目录）由消费方
   （`recommend-rank`、`bg/model_serving`）校验，坏产物拒绝加载。
