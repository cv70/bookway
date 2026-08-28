"""Evaluation metrics computed without scikit-learn: holdout logloss and a
Mann-Whitney AUC that tolerates ties."""

import math

import torch
from torch import nn


def logloss(model: nn.Module, features: torch.Tensor, labels: torch.Tensor) -> float:
    if features.shape[0] == 0:
        return float("nan")
    logits = model(features)
    loss = nn.functional.binary_cross_entropy_with_logits(
        logits, labels, reduction="mean"
    )
    return float(loss.item())


def auc(model: nn.Module, features: torch.Tensor, labels: torch.Tensor) -> float | None:
    positives = int(labels.sum().item())
    negatives = int(labels.shape[0] - positives)
    if positives == 0 or negatives == 0:
        return None
    with torch.no_grad():
        scores = torch.sigmoid(model(features))
    pairs = sorted(zip(scores.tolist(), labels.tolist()), key=lambda pair: pair[0])
    rank_sum = 0.0
    index = 0
    while index < len(pairs):
        same = index + 1
        while same < len(pairs) and pairs[same][0] == pairs[index][0]:
            same += 1
        average_rank = (index + 1 + same) / 2.0  # ties share the average rank
        for score, label in pairs[index:same]:
            if label:
                rank_sum += average_rank
        index = same
    return (rank_sum - positives * (positives + 1) / 2.0) / (positives * negatives)
