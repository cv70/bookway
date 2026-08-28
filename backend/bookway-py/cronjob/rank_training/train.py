"""Train the recommend-rank logistic artifact.

Closed loop: the exposure ledger records serving-time feature snapshots
(0085) -> this script fits per-objective logistic heads with a time-ordered
holdout -> the exported artifact JSON is served by `recommend-rank` via
`RECOMMEND_RANK_MODEL_ARTIFACT` -> new exposures (with the artifact's version
in the ledger) feed the next round. The job refuses to write an artifact when
a mandatory head has too few positives: an unevaluable model must not serve.

Usage:
    DATABASE_URL=... TRAINER_OUTPUT_PATH=rank-model-artifact.json \
        python train.py
"""

import json
import sys
import time

import psycopg
import torch
from torch import nn

from config import OBJECTIVES, TrainingConfig
from data import load_samples, time_ordered_split
from metrics import auc, logloss
from model import LogisticHead, feature_tensor

# The labelling event type per head, kept next to the fit loop for reading.
OBJECTIVES_LABELS = {name: event for name, event, _ in OBJECTIVES}


def fit_head(
    name: str,
    initial_bias: float,
    train_samples,
    config: TrainingConfig,
) -> LogisticHead:
    torch.manual_seed(config.seed)
    model = LogisticHead(initial_bias)
    optimizer = torch.optim.AdamW(
        model.parameters(), lr=config.learning_rate, weight_decay=config.l2
    )
    features = torch.stack([feature_tensor(sample.features) for sample in train_samples])
    labels = torch.tensor(
        [sample.label(OBJECTIVES_LABELS[name], config.attribution_days) for sample in train_samples],
        dtype=torch.float32,
    )
    model.train()
    for epoch in range(config.epochs):
        permutation = torch.randperm(features.shape[0])
        for start in range(0, features.shape[0], config.batch_size):
            batch = permutation[start : start + config.batch_size]
            optimizer.zero_grad()
            logits = model(features[batch])
            loss = nn.functional.binary_cross_entropy_with_logits(logits, labels[batch])
            loss.backward()
            optimizer.step()
        if (epoch + 1) % 100 == 0:
            print(
                f"  [{name}] epoch {epoch + 1}/{config.epochs} loss {loss.item():.6f}",
                flush=True,
            )
    model.eval()
    return model


def main() -> None:
    config = TrainingConfig()
    config.validate()

    with psycopg.connect(config.database_url) as conn:
        samples = load_samples(conn, config.label_window_days, config.attribution_days)
    if len(samples) < config.min_positives * 2:
        raise SystemExit(
            f"only {len(samples)} labelled samples in the window; "
            f"refusing to train (need at least {config.min_positives * 2})"
        )
    train_samples, holdout_samples = time_ordered_split(samples, config.holdout_fraction)
    print(
        f"samples: {len(samples)} (train {len(train_samples)} / holdout {len(holdout_samples)})"
    )

    artifact_heads: dict[str, dict] = {}
    report_heads: dict[str, dict] = {}
    for name, event_type, initial_bias in OBJECTIVES:
        train_labels = [
            sample.label(event_type, config.attribution_days) for sample in train_samples
        ]
        positives = sum(train_labels)
        if positives < config.min_positives:
            print(
                f"[{name}] skipped: {positives} positive labels, "
                f"{config.min_positives} required",
                flush=True,
            )
            report_heads[name] = {
                "samples": len(train_samples),
                "positives": positives,
                "trained": False,
                "skipped_reason": f"only {positives} positive labels",
            }
            continue

        print(f"[{name}] training on {len(train_samples)} samples ({positives} positive)")
        model = fit_head(name, initial_bias, train_samples, config)
        holdout_features = torch.stack(
            [feature_tensor(s.features) for s in holdout_samples]
        )
        holdout_labels = torch.tensor(
            [s.label(event_type, config.attribution_days) for s in holdout_samples],
            dtype=torch.float32,
        )
        holdout_logloss = logloss(model, holdout_features, holdout_labels)
        holdout_auc = auc(model, holdout_features, holdout_labels)
        print(
            f"[{name}] holdout logloss {holdout_logloss:.6f} "
            f"auc {holdout_auc if holdout_auc is not None else float('nan'):.4f}"
        )
        artifact_heads[name] = {
            "bias": model.bias(),
            "weights": model.weights(),
        }
        report_heads[name] = {
            "samples": len(train_samples),
            "positives": positives,
            "train_logloss": logloss(
                model,
                torch.stack([feature_tensor(s.features) for s in train_samples]),
                torch.tensor(train_labels, dtype=torch.float32),
            ),
            "holdout_logloss": holdout_logloss,
            "holdout_auc": holdout_auc,
            "trained": True,
        }

    if "ctr" not in artifact_heads:
        raise SystemExit(
            "the click head is mandatory: without it the fusion formula loses its floor"
        )
    # Untrained objectives export the prior bias with zero weights so the
    # serving contract keeps all three heads; predicting nothing honestly.
    for name, _, initial_bias in OBJECTIVES:
        artifact_heads.setdefault(name, {"bias": initial_bias, "weights": {}})

    version = f"lr-{time.strftime('%Y%m%d', time.gmtime())}"
    artifact = {
        "version": version,
        "bias": {name: artifact_heads[name]["bias"] for name, _, _ in OBJECTIVES},
        "weights": {name: artifact_heads[name]["weights"] for name, _, _ in OBJECTIVES},
    }
    with open(config.output_path, "w", encoding="utf-8") as handle:
        json.dump(artifact, handle, indent=2, ensure_ascii=False)
    report_path = config.output_path.removesuffix(".json") + ".report.json"
    with open(report_path, "w", encoding="utf-8") as handle:
        json.dump(
            {
                "version": version,
                "label_window_days": config.label_window_days,
                "attribution_days": config.attribution_days,
                "trained_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "heads": report_heads,
            },
            handle,
            indent=2,
        )
    print(
        f"artifact written to {config.output_path}; promote by pointing "
        "RECOMMEND_RANK_MODEL_ARTIFACT at it and restarting recommend-rank"
    )


if __name__ == "__main__":
    main()
