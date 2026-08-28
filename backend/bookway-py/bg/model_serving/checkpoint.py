"""Loading contract for the trained three-objective scoring head.

`bookway-py/cronjob/rank_training/train_llm.py` saves into one checkpoint
directory:
* a LoRA adapter under `adapter/`, and
* `scoring_head.pt` — an `nn.Linear(hidden, 3)` state dict.

`load_scorer` rebuilds the head for the serving app, which applies the
adapter to the shared backbone itself (`PeftModel.from_pretrained`). Both
halves are required: the head was trained on adapter-tuned hidden states,
so a checkpoint missing `adapter/` is rejected by the serving side's
`_valid_checkpoint` instead of being served as a silent model mismatch.
"""

import os

import torch
from torch import nn


class ScoringHead(nn.Module):
    def __init__(self, hidden_size: int) -> None:
        super().__init__()
        self.dropout = nn.Dropout(0.1)
        self.linear = nn.Linear(hidden_size, 3)

    def forward(self, pooled: torch.Tensor) -> torch.Tensor:
        return self.linear(self.dropout(pooled))


def load_scorer(path: str, hidden_size: int, device: str) -> ScoringHead:
    head = ScoringHead(hidden_size)
    state = torch.load(os.path.join(path, "scoring_head.pt"), map_location="cpu")
    head.load_state_dict(state)
    return head.to(device).eval()
