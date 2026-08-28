"""PyTorch multi-objective logistic heads matching the serving artifact.

One linear head per objective (pCTR / pCVR / pWEGU). Serving applies a
sigmoid to `bias + Σ w_i · x_i` over the same closed feature set, so this
module is intentionally boring: a learned replacement for the heuristic's
hand-set priors, upgradeable without touching Rust.
"""

import torch
from torch import nn

from config import FEATURE_NAMES


class LogisticHead(nn.Module):
    def __init__(self, initial_bias: float) -> None:
        super().__init__()
        self.linear = nn.Linear(len(FEATURE_NAMES), 1)
        nn.init.zeros_(self.linear.weight)
        nn.init.constant_(self.linear.bias, initial_bias)

    def forward(self, features: torch.Tensor) -> torch.Tensor:
        return self.linear(features).squeeze(-1)

    def bias(self) -> float:
        return float(self.linear.bias.item())

    def weights(self) -> dict[str, float]:
        return {
            name: float(weight)
            for name, weight in zip(FEATURE_NAMES, self.linear.weight[0].detach())
        }


def feature_tensor(features: tuple[float, ...]) -> torch.Tensor:
    return torch.tensor(features, dtype=torch.float32)
