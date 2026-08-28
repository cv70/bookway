"""Training configuration, all env-driven so CI and operators share one file.

The output artifact contract is fixed by the serving side
(`recommend-rank`'s LinearPredictor): `{version, bias, weights}` where each
head weights a CLOSED set of serving-time features. Feature names must match
`rank/algorithm.rs`'s evidence snapshot — unknown names are rejected at
serving startup, which is the loop's typo firewall.
"""

import os
from dataclasses import dataclass, field

# The trainer contract mirrors recommend-rank's ObjectiveEvidence exactly.
FEATURE_NAMES: tuple[str, ...] = (
    "explicit_ctr",
    "observed_ctr",
    "observed_cvr",
    "observed_wegu",
    "route_completion",
    "domain_affinity",
    "author_affinity",
    "impression_fatigue",
    "direct_negative_feedback",
)

# (head name, labelling event type, initial bias from the objective prior).
OBJECTIVES: tuple[tuple[str, str, float], ...] = (
    ("ctr", "click", -3.2),
    ("cvr", "purchase", -4.6),
    ("wegu", "complete", -2.5),
)


def _int_env(key: str, default: int) -> int:
    value = os.environ.get(key, "").strip()
    return int(value) if value else default


def _float_env(key: str, default: float) -> float:
    value = os.environ.get(key, "").strip()
    return float(value) if value else default


@dataclass
class TrainingConfig:
    database_url: str = field(
        default_factory=lambda: os.environ.get("DATABASE_URL", "")
    )
    output_path: str = field(
        default_factory=lambda: os.environ.get(
            "TRAINER_OUTPUT_PATH", "rank-model-artifact.json"
        )
    )
    label_window_days: int = field(
        default_factory=lambda: _int_env("TRAINER_LABEL_WINDOW_DAYS", 28)
    )
    attribution_days: int = field(
        default_factory=lambda: _int_env("TRAINER_ATTRIBUTION_WINDOW_DAYS", 7)
    )
    min_positives: int = field(
        default_factory=lambda: _int_env("TRAINER_MIN_POSITIVES", 200)
    )
    epochs: int = field(default_factory=lambda: _int_env("TRAINER_EPOCHS", 400))
    learning_rate: float = field(
        default_factory=lambda: _float_env("TRAINER_LEARNING_RATE", 0.08)
    )
    l2: float = field(default_factory=lambda: _float_env("TRAINER_L2", 1e-4))
    holdout_fraction: float = field(
        default_factory=lambda: _float_env("TRAINER_HOLDOUT_FRACTION", 0.2)
    )
    batch_size: int = field(default_factory=lambda: _int_env("TRAINER_BATCH_SIZE", 512))
    seed: int = field(default_factory=lambda: _int_env("TRAINER_SEED", 20260827))

    def validate(self) -> None:
        if not self.database_url:
            raise SystemExit("DATABASE_URL is required")
        if not 0.0 < self.holdout_fraction < 1.0:
            raise SystemExit("TRAINER_HOLDOUT_FRACTION must be in (0, 1)")
