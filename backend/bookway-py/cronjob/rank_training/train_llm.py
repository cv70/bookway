"""LoRA fine-tune of MiniCPM5-1B as the three-objective recommendation scorer.

Serving contract (backend/bookway-py/bg/model_serving/app.py `/score`):
    user_context + candidate text -> backbone mean pooling -> ScoringHead
    (nn.Linear(hidden, 3)) -> sigmoid -> (pCTR, pCVR, pWEGU).

Training data comes from the exposure ledger (migration 0085 feature
snapshots serve the logistic artifact path; the LLM consumes TEXT): the
ledger supplies attribution labels and identities, bbs-link supplies the
candidate text, and the user interest context is reconstructed from each
user's PRE-exposure events — exactly the facts the online ranker can
legitimately see at serving time. Nothing post-exposure leaks into the
prompt.

Guardrails: time-ordered holdout; per-objective AUC reported; the run
refuses when samples are too few — an unevaluable model must not reach
serving. Publishing follows the same principle: the checkpoint directory is
always kept for offline inspection, but the serving registry
(TRAINER_REGISTRY_PATH, hot-reloaded by model_serving) is written only when
every evaluable holdout AUC clears TRAINER_LLM_MIN_AUC. model_serving keeps
`ready: false` until a published checkpoint exists.

Usage (GPU expected; base model ~2-3 GB downloaded via ModelScope):
    DATABASE_URL=... TRAINER_LLM_OUTPUT_DIR=/opt/bookway/models/minicpm-ranker \
    TRAINER_REGISTRY_PATH=/opt/bookway/models/ranker-registry.json \
    python train_llm.py
"""

from __future__ import annotations

import json
import os
import sys
import time
from dataclasses import dataclass

import psycopg
import torch
from torch import nn

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from config import _float_env, _int_env  # noqa: E402

MODEL_NAME = os.environ.get("MODEL_NAME", "OpenBMB/MiniCPM5-1B")
MODEL_SOURCE = os.environ.get("MODEL_SOURCE", "modelscope")
OUTPUT_DIR = os.environ.get("TRAINER_LLM_OUTPUT_DIR", "minicpm-ranker-output")
LABEL_WINDOW_DAYS = _int_env("TRAINER_LABEL_WINDOW_DAYS", 28)
ATTRIBUTION_DAYS = _int_env("TRAINER_ATTRIBUTION_WINDOW_DAYS", 7)
MIN_POSITIVES = _int_env("TRAINER_MIN_POSITIVES", 200)
MAX_TRAIN_SAMPLES = _int_env("TRAINER_MAX_TRAIN_SAMPLES", 20_000)
EPOCHS = _int_env("TRAINER_LLM_EPOCHS", 2)
LEARNING_RATE = _float_env("TRAINER_LLM_LEARNING_RATE", 1e-4)
LORA_R = _int_env("TRAINER_LORA_R", 8)
LORA_ALPHA = _int_env("TRAINER_LORA_ALPHA", 16)
BATCH_SIZE = _int_env("TRAINER_LLM_BATCH_SIZE", 8)
MAX_TEXT_CHARS = 512
DATABASE_URL = os.environ.get("DATABASE_URL", "")
REGISTRY_PATH = os.environ.get("TRAINER_REGISTRY_PATH", "").strip()
MIN_AUC = _float_env("TRAINER_LLM_MIN_AUC", 0.55)

OBJECTIVES: tuple[tuple[str, str], ...] = (
    ("ctr", "click"),
    ("cvr", "purchase"),
    ("wegu", "complete"),
)
DOMAIN_LABELS = {1: "学习", 2: "运动", 3: "健康", 4: "旅行", 5: "休闲"}


@dataclass(frozen=True)
class TrainingRow:
    user_id: str
    content_id: str
    user_context: str
    candidate_text: str
    labels: tuple[int, int, int]  # ctr, cvr, wegu
    at: float


def load_rows(conn: psycopg.Connection) -> list[TrainingRow]:
    with conn.cursor() as cursor:
        cursor.execute(
            """
            SELECT e.user_id, i.content_id,
                   EXTRACT(epoch FROM e.created_at)::float8
            FROM feed_exposure_items i
            JOIN feed_exposures e ON e.request_id = i.request_id
            WHERE e.created_at >= now() - make_interval(days => %s)
            ORDER BY e.created_at ASC
            LIMIT %s
            """,
            (LABEL_WINDOW_DAYS, MAX_TRAIN_SAMPLES * 4),
        )
        exposures = cursor.fetchall()

        cursor.execute(
            """
            SELECT user_id, content_id, event_type,
                   EXTRACT(epoch FROM occurred_at)::float8
            FROM user_events
            WHERE event_type IN ('click', 'purchase', 'complete')
              AND content_id IS NOT NULL
              AND occurred_at >= now() - make_interval(days => %s)
            """,
            (LABEL_WINDOW_DAYS + ATTRIBUTION_DAYS,),
        )
        events = cursor.fetchall()

        # Candidate text lives in bbs-link's content fact table.
        cursor.execute(
            """
            SELECT id, domain,
                   payload->'post'->>'title' AS title,
                   payload->'post'->>'summary' AS summary
            FROM content_items
            WHERE deleted_at IS NULL
            """
        )
        contents = cursor.fetchall()

    content_text: dict[str, tuple[str, str, int]] = {}
    for content_id, domain, title, summary in contents:
        content_text[content_id] = (
            (title or "")[:MAX_TEXT_CHARS],
            (summary or "")[:MAX_TEXT_CHARS],
            int(domain or 0),
        )

    events_by_user: dict[str, list[tuple[str, str, float]]] = {}
    for user_id, content_id, event_type, at in events:
        events_by_user.setdefault(user_id, []).append((content_id, event_type, at))

    domain_by_content = {
        content_id: domain for content_id, (_, _, domain) in content_text.items()
    }

    rows: list[TrainingRow] = []
    for user_id, content_id, at in exposures:
        meta = content_text.get(content_id)
        if not meta:
            continue
        title, summary, _ = meta
        if not title:
            continue

        # User interest text: domains of this user's PRE-exposure events,
        # weighted like the online composition (complete > click).
        domain_counts: dict[str, int] = {}
        for event_content, event_type, event_at in events_by_user.get(user_id, ()):
            if event_at >= at:
                continue
            domain = DOMAIN_LABELS.get(domain_by_content.get(event_content, 0))
            if domain:
                weight = 2 if event_type == "complete" else 1
                domain_counts[domain] = domain_counts.get(domain, 0) + weight
        interests = sorted(domain_counts, key=domain_counts.get, reverse=True)[:3]
        interests_text = "、".join(interests) if interests else "暂无明确兴趣"
        user_context = f"用户兴趣领域：{interests_text}；内容场景：发现流"

        labels: list[int] = []
        for _, event_type in OBJECTIVES:
            deadline = at + ATTRIBUTION_DAYS * 86_400.0
            labels.append(
                int(
                    any(
                        pair_content == content_id
                        and pair_type == event_type
                        and at <= pair_at <= deadline
                        for pair_content, pair_type, pair_at in events_by_user.get(
                            user_id, []
                        )
                    )
                )
            )
        rows.append(
            TrainingRow(
                user_id=user_id,
                content_id=content_id,
                user_context=user_context,
                candidate_text=f"{title}。{summary}",
                labels=(labels[0], labels[1], labels[2]),
                at=at,
            )
        )
    return rows


def resolve_model_path() -> str:
    if MODEL_SOURCE == "modelscope":
        from modelscope import snapshot_download

        return snapshot_download(MODEL_NAME)
    return MODEL_NAME


def publish(
    checkpoint_dir: str,
    holdout_auc: dict[str, float | None],
    train_rows: int,
    holdout_rows: int,
) -> None:
    """Auto-publish: gate the registry on the holdout, write it atomically.

    model_serving hot-reloads TRAINER_REGISTRY_PATH (its MODEL_REGISTRY_PATH)
    on every /score call, so this file — not the checkpoint directory — is
    the switch that puts a model into serving. A run whose evaluable holdout
    AUC misses TRAINER_LLM_MIN_AUC exits non-zero with the checkpoint kept
    for inspection; the registry keeps pointing at the previous model.
    """
    if not REGISTRY_PATH:
        print("TRAINER_REGISTRY_PATH unset: checkpoint kept, registry not published")
        return
    evaluable = {name: auc for name, auc in holdout_auc.items() if auc is not None}
    if not evaluable:
        raise SystemExit(
            f"publish refused: no evaluable holdout objective (auc={holdout_auc})"
        )
    failures = {name: round(auc, 4) for name, auc in evaluable.items() if auc < MIN_AUC}
    if failures:
        raise SystemExit(
            f"publish refused: holdout AUC below gate {MIN_AUC}: {failures}"
        )
    entry = {
        "schema_version": 1,
        "checkpoint_path": os.path.abspath(checkpoint_dir),
        "base_model": MODEL_NAME,
        "trained_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "train_rows": train_rows,
        "holdout_rows": holdout_rows,
        "holdout_auc": holdout_auc,
        "min_auc_gate": MIN_AUC,
    }
    os.makedirs(os.path.dirname(os.path.abspath(REGISTRY_PATH)), exist_ok=True)
    tmp = REGISTRY_PATH + ".tmp"
    with open(tmp, "w", encoding="utf-8") as handle:
        json.dump(entry, handle, indent=2, ensure_ascii=False)
    os.replace(tmp, REGISTRY_PATH)  # atomic: hot reload never sees a half-written registry
    print(
        f"registry published: {REGISTRY_PATH} -> {entry['checkpoint_path']} "
        f"(holdout AUC {holdout_auc})"
    )


def main() -> None:
    if not DATABASE_URL:
        raise SystemExit("DATABASE_URL is required")
    try:
        from peft import LoraConfig, TaskType, get_peft_model
        from transformers import AutoModel, AutoTokenizer
    except ImportError:
        raise SystemExit(
            "LLM training needs transformers+peft: "
            "pip install -r ../bg/model_serving/requirements.txt peft"
        )

    torch.manual_seed(_int_env("TRAINER_SEED", 20260827))
    with psycopg.connect(DATABASE_URL) as conn:
        rows = load_rows(conn)
    if len(rows) < MIN_POSITIVES * 2:
        raise SystemExit(
            f"only {len(rows)} training rows; refusing (need {MIN_POSITIVES * 2})"
        )

    split = len(rows) - max(1, int(len(rows) * 0.2))
    train_rows, holdout_rows = rows[:split], rows[split:]
    print(f"rows: {len(rows)} (train {len(train_rows)} / holdout {len(holdout_rows)})")

    model_path = resolve_model_path()
    tokenizer = AutoTokenizer.from_pretrained(model_path, trust_remote_code=True)
    backbone = AutoModel.from_pretrained(model_path, trust_remote_code=True)
    hidden = int(backbone.config.hidden_size)

    lora = LoraConfig(
        task_type=TaskType.FEATURE_EXTRACTION,
        r=LORA_R,
        lora_alpha=LORA_ALPHA,
        lora_dropout=0.05,
        target_modules=["q_proj", "v_proj"],
    )
    backbone = get_peft_model(backbone, lora)
    backbone.print_trainable_parameters()

    head = nn.Linear(hidden, 3)
    nn.init.zeros_(head.weight)
    nn.init.constant_(head.bias, -2.5)

    device = "cuda" if torch.cuda.is_available() else "cpu"
    backbone.to(device)
    head.to(device)
    trainable = [p for p in backbone.parameters() if p.requires_grad] + list(
        head.parameters()
    )
    optimizer = torch.optim.AdamW(trainable, lr=LEARNING_RATE)

    def encode(rows_subset: list[TrainingRow]):
        texts = [
            f"{row.user_context}\n[SEP]\n{row.candidate_text}"[:MAX_TEXT_CHARS]
            for row in rows_subset
        ]
        encoded = tokenizer(texts, padding=True, truncation=True, return_tensors="pt")
        labels = torch.tensor([row.labels for row in rows_subset], dtype=torch.float32)
        return encoded.to(device), labels.to(device)

    def pooled_hidden(encoded) -> torch.Tensor:
        output = backbone(**encoded, return_dict=True)
        last_hidden = output[0].float()
        mask = encoded["attention_mask"].unsqueeze(-1).to(last_hidden.dtype)
        return (last_hidden * mask).sum(1) / mask.sum(1).clamp(min=1e-9)

    def holdout_auc_per_objective() -> dict[str, float | None]:
        backbone.eval()
        head.eval()
        encoded, labels = encode(holdout_rows)
        with torch.no_grad():
            scores = torch.sigmoid(head(pooled_hidden(encoded)))
        results: dict[str, float | None] = {}
        for index, (name, _) in enumerate(OBJECTIVES):
            positives = int(labels[:, index].sum().item())
            if positives in (0, len(labels)):
                results[name] = None
                continue
            preds = scores[:, index].tolist()
            truth = labels[:, index].tolist()
            order = torch.argsort(scores[:, index]).tolist()
            ranks = [0] * len(order)
            for rank, row_index in enumerate(order):
                ranks[row_index] = rank
            results[name] = _auc_from_ranks(ranks, truth)
        return results

    def _auc_from_ranks(ranks: list[int], labels: list[int]) -> float:
        positives = [rank for rank, label in zip(ranks, labels) if label]
        negatives = [rank for rank, label in zip(ranks, labels) if not label]
        if not positives or not negatives:
            return float("nan")
        wins = sum(1 for p in positives for n in negatives if p > n)
        ties = sum(1 for p in positives for n in negatives if p == n)
        return (wins + 0.5 * ties) / (len(positives) * len(negatives))

    print(f"training on {device}: {EPOCHS} epochs, LoRA r={LORA_R}")
    for epoch in range(EPOCHS):
        backbone.train()
        head.train()
        order = torch.randperm(len(train_rows))
        total_loss = 0.0
        for start in range(0, len(train_rows), BATCH_SIZE):
            batch_indices = order[start : start + BATCH_SIZE].tolist()
            batch_rows = [train_rows[i] for i in batch_indices]
            encoded, labels = encode(batch_rows)
            optimizer.zero_grad()
            scores = head(pooled_hidden(encoded))
            loss = nn.functional.binary_cross_entropy_with_logits(scores, labels)
            loss.backward()
            optimizer.step()
            total_loss += float(loss.item()) * len(batch_rows)
        print(
            f"epoch {epoch + 1}/{EPOCHS} loss {total_loss / len(train_rows):.4f} "
            f"holdout AUC {holdout_auc_per_objective()}",
            flush=True,
        )

    final_auc = holdout_auc_per_objective()

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    torch.save(head.state_dict(), os.path.join(OUTPUT_DIR, "scoring_head.pt"))
    backbone.save_pretrained(os.path.join(OUTPUT_DIR, "adapter"))
    with open(
        os.path.join(OUTPUT_DIR, "training_meta.json"), "w", encoding="utf-8"
    ) as handle:
        json.dump(
            {
                "base_model": MODEL_NAME,
                "label_window_days": LABEL_WINDOW_DAYS,
                "attribution_days": ATTRIBUTION_DAYS,
                "trained_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "train_rows": len(train_rows),
                "holdout_rows": len(holdout_rows),
                "holdout_auc": final_auc,
            },
            handle,
            indent=2,
            ensure_ascii=False,
        )
    publish(OUTPUT_DIR, final_auc, len(train_rows), len(holdout_rows))
    print(f"checkpoint written to {OUTPUT_DIR}")


if __name__ == "__main__":
    main()
