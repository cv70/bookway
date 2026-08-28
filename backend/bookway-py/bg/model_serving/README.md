# model_serving — MiniCPM5-1B 推理服务（常驻）

推荐模型基模的独立推理服务（对应 x-algorithm 中 media-model-proxy /
simclustersann 的角色）：Rust 在线链路不直接加载模型，一切推理经 HTTP。

## 端点

| 端点 | 鉴权 | 说明 |
| --- | --- | --- |
| `GET /health` | 内网 | 加载状态、hidden size、scorer 解析结果（来源：registry 或静态路径） |
| `POST /v1/embeddings` | 内网 | OpenAI 兼容；**基模** mean-pooling embeddings（LoRA 适配器禁用，与索引期维度/校准一致），**下载即用、无需训练** |
| `POST /score` | 内网 | 三目标打分（pCTR/pCVR/pWEGU）；**checkpoint 门控**——无有效产物时返回 `{"ready": false}`，recommend-rank 据此降级启发式，绝不编造分数 |

## Checkpoint 契约（两半缺一不可）

`train_llm.py` 产物目录必须同时包含 `scoring_head.pt` 与 `adapter/`（LoRA）。
打分头是在适配器微调后的隐状态上训练的，只有 head 没有 adapter 属于模型错配，
`_valid_checkpoint` 直接拒绝，不会"降级近似"。加载后的适配器只作用于 `/score`；
`/v1/embeddings` 始终在 `disable_adapter()` 下跑基模。

## Checkpoint 解析与热加载（自动发布）

`/score` 每次调用先按序解析 checkpoint，稳定态开销仅一次 stat + 字典比较：

1. `MODEL_REGISTRY_PATH` — 训练作业通过门控后原子发布的注册表 JSON
   （`train_llm.py` 的 `TRAINER_REGISTRY_PATH`，两侧指向同一文件）；
2. `MODEL_CHECKPOINT_PATH` — 运维静态指定的 checkpoint 目录。

注册表更新后**无需重启**即生效（scorer 按 checkpoint 路径缓存、变更即重载）；
`model_version` 随响应返回，recommend-rank 将其写入曝光账本留痕。

## 运行

```bash
pip install -r requirements.txt
MODEL_NAME=OpenBMB/MiniCPM5-1B MODEL_SOURCE=modelscope ./run.sh
```

首次启动经 ModelScope 下载基模快照（~2-3 GB），缓存后离线可用；
`MODEL_DIR` 可指向预下载目录，`MODEL_CHECKPOINT_PATH` / `MODEL_REGISTRY_PATH`
指向 `../../cronjob/rank_training/train_llm.py` 的产物（见上方契约）。

本地验证（离线冒烟，无需下载基模/数据库）：

```bash
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt httpx
.venv/bin/python smoke_test.py   # 用程序化生成的微型模型验证完整服务契约
```

## 接线

| 消费方 | 配置 | 效果 |
| --- | --- | --- |
| knowledge-catalog | `RAG_EMBEDDING_ENDPOINT=http://127.0.0.1:8110/v1/embeddings`、`RAG_EMBEDDING_MODEL=OpenBMB/MiniCPM5-1B` | RAG 节点问答即刻用基模向量 |
| bbs-indexer | `KNOWLEDGE_CATALOG_GRPC_URL` 已有 + `SEMANTIC_VECTOR_DIMS=<hidden size>` | 路线/节点/装备语义搜索的向量来源 |
| recommend-rank | `RECOMMEND_RANK_MODEL_ENDPOINT=http://127.0.0.1:8110` | LLM 打分参与融合排序（未就绪自动降级） |

维度约束：`SEMANTIC_VECTOR_DIMS` 必须等于基模 hidden size（写入索引后不可改）。
MiniCPM5-1B 实测 hidden size = **1536**（本机已下载快照并过 `real_model_check.py`：
8/8，相关文本余弦 0.904 > 无关 0.817，无 checkpoint 时 `/score` 如实 `ready:false`）。
