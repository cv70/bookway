"""Exposure + label extraction from the PostgreSQL behavior ledger.

Training reads SERVING-TIME feature snapshots from `feed_exposure_items`
(migration 0085) — never re-derived aggregates, which would let behavior that
happened after an impression leak into that impression's features. Labels are
click/purchase/complete events attributed to the (user, content) pair only
when they happen AFTER the impression and inside the attribution window.
"""

from dataclasses import dataclass

import psycopg

from config import FEATURE_NAMES


@dataclass(frozen=True)
class Sample:
    exposure_epoch: float
    features: tuple[float, ...]
    events: tuple[tuple[str, float], ...]

    def label(self, event_type: str, attribution_days: int) -> int:
        deadline = self.exposure_epoch + attribution_days * 86_400.0
        return int(
            any(
                kind == event_type and self.exposure_epoch <= at <= deadline
                for kind, at in self.events
            )
        )


def _features_from_snapshot(snapshot: dict) -> tuple[float, ...]:
    return tuple(
        float(snapshot[name]) if isinstance(snapshot.get(name), (int, float)) else 0.0
        for name in FEATURE_NAMES
    )


def load_samples(conn: psycopg.Connection, label_window_days: int, attribution_days: int) -> list[Sample]:
    with conn.cursor() as cursor:
        cursor.execute(
            """
            SELECT e.user_id, i.content_id, i.feature_snapshot,
                   EXTRACT(epoch FROM e.created_at)::float8
            FROM feed_exposure_items i
            JOIN feed_exposures e ON e.request_id = i.request_id
            WHERE i.feature_snapshot <> '{}'::jsonb
              AND e.created_at >= now() - make_interval(days => %s)
            ORDER BY e.created_at ASC
            """,
            (label_window_days,),
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
            (label_window_days + attribution_days,),
        )
        events = cursor.fetchall()

    by_key: dict[tuple[str, str], list[tuple[str, float]]] = {}
    for user_id, content_id, event_type, at in events:
        by_key.setdefault((user_id, content_id), []).append((event_type, at))

    return [
        Sample(
            exposure_epoch=at,
            features=_features_from_snapshot(snapshot),
            events=tuple(by_key.get((user_id, content_id), ())),
        )
        for user_id, content_id, snapshot, at in exposures
    ]


def time_ordered_split(
    samples: list[Sample], holdout_fraction: float
) -> tuple[list[Sample], list[Sample]]:
    """Newest slice becomes the holdout; training data is always the past."""
    if not samples:
        return [], []
    holdout_size = max(1, int(len(samples) * holdout_fraction))
    split = len(samples) - holdout_size
    return samples[:split], samples[split:]
