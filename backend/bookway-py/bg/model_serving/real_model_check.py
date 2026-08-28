"""Real-model verification for model_serving against a downloaded snapshot.

Unlike smoke_test.py (tiny fixture, pure contract), this boots the service on
an actual MiniCPM snapshot and checks the qualities that only a real model
has: meaningful embedding geometry (related texts embed closer than
unrelated ones), consistent hidden size, and honest scorer gating.

Usage (snapshot path from modelscope snapshot_download or MODEL_DIR):
    .venv/bin/python real_model_check.py /path/to/snapshot
"""

from __future__ import annotations

import math
import os
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, ROOT)


def cosine(a: list[float], b: list[float]) -> float:
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(x * x for x in b))
    return dot / (na * nb)


def main() -> int:
    snapshot = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("MODEL_DIR", "")
    if not snapshot or not os.path.isdir(snapshot):
        print("usage: real_model_check.py <snapshot-dir>")
        return 2
    os.environ.update(
        MODEL_DIR=snapshot,
        MODEL_SOURCE="hf",
        MODEL_DEVICE=os.environ.get("MODEL_DEVICE", "cpu"),
        MODEL_CHECKPOINT_PATH="",
        MODEL_REGISTRY_PATH="",
    )
    import app as serving
    from fastapi.testclient import TestClient

    checks: list[str] = []

    def check(name: str, condition: bool) -> None:
        checks.append(f"{'PASS' if condition else 'FAIL'} {name}")
        print(checks[-1], flush=True)

    with TestClient(serving.app) as client:
        health = client.get("/health").json()
        check("base model loaded from real snapshot", health["status"] == "ok")
        hidden = health["hidden_size"] or 0
        check(f"hidden size is plausible ({hidden})", hidden >= 256)
        check(
            "no checkpoint -> scorer honestly not ready",
            health["scorer_ready"] is False,
        )

        texts = [
            "户外徒步前必须检查装备清单和天气",
            "登山鞋和防水外套是徒步的核心装备",
            "本周的数学课程重点讲解了线性代数",
        ]
        embedded = client.post("/v1/embeddings", json={"input": texts}).json()
        vectors = [row["embedding"] for row in embedded["data"]]
        check("embeddings returned for every text", len(vectors) == 3)
        check("embedding dim == hidden size", all(len(v) == hidden for v in vectors))
        related = cosine(vectors[0], vectors[1])
        unrelated = cosine(vectors[0], vectors[2])
        check(
            f"geometry: related {related:.3f} > unrelated {unrelated:.3f}",
            related > unrelated,
        )
        again = client.post("/v1/embeddings", json={"input": texts}).json()
        check(
            "embeddings deterministic",
            vectors == [row["embedding"] for row in again["data"]],
        )
        score = client.post("/score", json={"items": [{"user_context": "u", "candidate_text": "c"}]}).json()
        check("untrained scorer returns ready:false", score["ready"] is False)

    failed = [line for line in checks if line.startswith("FAIL")]
    print(f"\n{len(checks) - len(failed)}/{len(checks)} checks passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
