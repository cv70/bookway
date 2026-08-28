"""Offline smoke test for model_serving — full service contract, no GPU/DB/download.

Builds a tiny random-weight backbone + tokenizer programmatically, then
exercises the FastAPI app through TestClient:

* honest gating: no checkpoint / head-only checkpoint / dead registry -> ``ready: false``
* two-half contract: adapter + scoring head both required, adapter ACTUALLY
  changes /score output (endpoint results must match a reference PeftModel)
* embeddings stay on the base model even while a scorer is loaded
  (``disable_adapter``) — vectors must equal a pristine base reference
* registry hot reload: republish -> new model_version + new scores, no restart
* corrupt registry falls back to the static path, never crashes

This verifies the CONTRACT, not model quality — training lives in
cronjob/rank_training. Run: ``.venv/bin/python smoke_test.py``.
"""

from __future__ import annotations

import json
import os
import shutil
import sys
import tempfile

ROOT = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, ROOT)

import torch
from peft import LoraConfig, TaskType, get_peft_model
from tokenizers import Tokenizer, models, pre_tokenizers
from transformers import AutoModel, AutoTokenizer, LlamaConfig, LlamaModel, PreTrainedTokenizerFast

from checkpoint import ScoringHead  # serving's own loading contract

HIDDEN = 32
VOCAB = [
    "[UNK]", "[PAD]", "[SEP]",
    "用户", "兴趣", "领域", "学习", "旅行", "内容", "路线", "装备", "场景",
]
ITEMS = [
    {"user_context": "用户 兴趣 领域：学习", "candidate_text": "学习 路线 内容"},
    {"user_context": "用户 兴趣 领域：旅行", "candidate_text": "旅行 装备 场景"},
]


def build_base(path: str) -> str:
    config = LlamaConfig(
        hidden_size=HIDDEN,
        intermediate_size=64,
        num_hidden_layers=2,
        num_attention_heads=4,
        vocab_size=len(VOCAB),
        max_position_embeddings=64,
    )
    LlamaModel(config).save_pretrained(path)
    tok = Tokenizer(
        models.WordLevel(vocab={word: index for index, word in enumerate(VOCAB)}, unk_token="[UNK]")
    )
    tok.pre_tokenizer = pre_tokenizers.Whitespace()
    fast = PreTrainedTokenizerFast(
        tokenizer_object=tok, unk_token="[UNK]", pad_token="[PAD]", sep_token="[SEP]"
    )
    fast.model_max_length = 64
    fast.save_pretrained(path)
    return path


def build_checkpoint(base_path: str, ckpt_dir: str, seed: int) -> None:
    """Fixture produced by the same calls train_llm.py makes — not a real model."""
    torch.manual_seed(seed)
    backbone = AutoModel.from_pretrained(base_path)
    peft_model = get_peft_model(
        backbone,
        LoraConfig(
            task_type=TaskType.FEATURE_EXTRACTION,
            r=4,
            lora_alpha=8,
            lora_dropout=0.0,
            target_modules=["q_proj", "v_proj"],
        ),
    )
    peft_model.save_pretrained(os.path.join(ckpt_dir, "adapter"))
    torch.save(ScoringHead(HIDDEN).state_dict(), os.path.join(ckpt_dir, "scoring_head.pt"))


def _pooled(model, tokenizer, texts: list[str]) -> torch.Tensor:
    encoded = tokenizer(texts, padding=True, return_tensors="pt")
    with torch.no_grad():
        output = model(**encoded, return_dict=True)
    last_hidden = output[0].float()
    mask = encoded["attention_mask"].unsqueeze(-1).to(last_hidden.dtype)
    return (last_hidden * mask).sum(1) / mask.sum(1).clamp(min=1e-9)


def reference_scores(base_path: str, ckpt_dir: str) -> list[dict]:
    """Mirror of the serving path: PeftModel adapter + head + sigmoid."""
    from peft import PeftModel

    model = PeftModel.from_pretrained(
        AutoModel.from_pretrained(base_path), os.path.join(ckpt_dir, "adapter")
    ).eval()
    head = ScoringHead(HIDDEN).eval()
    head.load_state_dict(torch.load(os.path.join(ckpt_dir, "scoring_head.pt"), map_location="cpu"))
    tokenizer = AutoTokenizer.from_pretrained(base_path)
    texts = [f"{item['user_context']}\n[SEP]\n{item['candidate_text']}" for item in ITEMS]
    scores = torch.sigmoid(head(_pooled(model, tokenizer, texts))).tolist()
    return [{"p_ctr": row[0], "p_cvr": row[1], "p_wegu": row[2]} for row in scores]


def reference_embeddings(base_path: str, texts: list[str]) -> list[list[float]]:
    """Pristine base model — what /v1/embeddings must return regardless of scorer state."""
    model = AutoModel.from_pretrained(base_path).eval()
    tokenizer = AutoTokenizer.from_pretrained(base_path)
    return _pooled(model, tokenizer, texts).tolist()


def approx(first: list[dict], second: list[dict], tolerance: float = 1e-4) -> bool:
    for row_a, row_b in zip(first, second):
        for key in ("p_ctr", "p_cvr", "p_wegu"):
            if abs(row_a[key] - row_b[key]) > tolerance:
                return False
    return True


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="bookway-serving-smoke-")
    base_path = build_base(os.path.join(tmp, "base"))
    ckpt_a = os.path.join(tmp, "ckpt-a")
    ckpt_b = os.path.join(tmp, "ckpt-b")
    head_only = os.path.join(tmp, "head-only")
    os.makedirs(head_only, exist_ok=True)
    build_checkpoint(base_path, ckpt_a, seed=1)
    build_checkpoint(base_path, ckpt_b, seed=2)
    shutil.copy(os.path.join(ckpt_a, "scoring_head.pt"), head_only)  # contract violation fixture
    registry_path = os.path.join(tmp, "registry.json")

    os.environ.update(
        MODEL_DIR=base_path,
        MODEL_SOURCE="hf",
        MODEL_DEVICE="cpu",
        MODEL_CHECKPOINT_PATH=head_only,
        MODEL_REGISTRY_PATH=registry_path,
    )
    import app as serving
    from fastapi.testclient import TestClient

    checks: list[str] = []

    def check(name: str, condition: bool) -> None:
        checks.append(f"{'PASS' if condition else 'FAIL'} {name}")
        if not condition:
            print(checks[-1], flush=True)

    def write_registry(checkpoint: str | None) -> None:
        if checkpoint is None:
            os.path.exists(registry_path) and os.remove(registry_path)
            return
        with open(registry_path, "w", encoding="utf-8") as handle:
            json.dump({"schema_version": 1, "checkpoint_path": checkpoint}, handle)

    with TestClient(serving.app) as client:  # context manager runs the lifespan loader
        health = client.get("/health").json()
        check("base model loaded from MODEL_DIR", health["status"] == "ok")
        check("hidden size exposed for SEMANTIC_VECTOR_DIMS", health["hidden_size"] == HIDDEN)
        check("head-only checkpoint refused (no adapter)", health["scorer_ready"] is False)

        score = client.post("/score", json={"items": ITEMS}).json()
        check("no valid checkpoint -> ready:false, no invented scores",
              score["ready"] is False and score["scores"] == [])

        # Registry publishes ckpt-a: adapter must change scores to the reference.
        write_registry(ckpt_a)
        score_a = client.post("/score", json={"items": ITEMS}).json()
        check("registry checkpoint served", score_a["ready"] is True)
        check("model_version carries checkpoint", score_a["model_version"].endswith(os.path.basename(ckpt_a)))
        check("scores match adapter+head reference", approx(score_a["scores"], reference_scores(base_path, ckpt_a)))
        check("scores are probabilities",
              all(0.0 <= row[key] <= 1.0 for row in score_a["scores"] for key in ("p_ctr", "p_cvr", "p_wegu")))

        # Embeddings: base-model contract, adapter disabled even with scorer loaded.
        texts = ["用户 学习 路线", "旅行 装备 场景"]
        embedded = client.post("/v1/embeddings", json={"input": texts}).json()
        vectors = [row["embedding"] for row in embedded["data"]]
        expected = reference_embeddings(base_path, texts)
        max_delta = max(abs(a - b) for va, vb in zip(vectors, expected) for a, b in zip(va, vb))
        check("embeddings equal pristine base (adapter disabled)", max_delta < 1e-4)
        check("embedding dim == hidden size", len(vectors[0]) == HIDDEN)
        again = client.post("/v1/embeddings", json={"input": texts}).json()
        check("embeddings deterministic", vectors == [row["embedding"] for row in again["data"]])

        # Hot reload: republish ckpt-b, no restart.
        write_registry(ckpt_b)
        score_b = client.post("/score", json={"items": ITEMS}).json()
        check("hot reload switches model_version", score_b["model_version"].endswith(os.path.basename(ckpt_b)))
        check("hot reload changes scores", not approx(score_a["scores"], score_b["scores"], tolerance=1e-3))
        check("hot reload matches new reference", approx(score_b["scores"], reference_scores(base_path, ckpt_b)))
        reloaded_vectors = [
            row["embedding"]
            for row in client.post("/v1/embeddings", json={"input": texts}).json()["data"]
        ]
        reloaded_delta = max(
            abs(a - b)
            for va, vb in zip(reloaded_vectors, expected)
            for a, b in zip(va, vb)
        )
        check("embeddings still pristine after adapter swap", reloaded_delta < 1e-4)
        loaded = client.get("/health").json()["scorer"]["loaded"]
        check("health reports registry source", loaded["source"] == "registry")

        # Broken registry must never crash or half-serve.
        write_registry(os.path.join(tmp, "missing"))
        check("registry -> missing checkpoint falls back to static (head-only, refused)",
              client.post("/score", json={"items": ITEMS}).json()["ready"] is False)
        with open(registry_path, "w", encoding="utf-8") as handle:
            handle.write("{not json")
        check("corrupt registry falls back, no crash",
              client.post("/score", json={"items": ITEMS}).json()["ready"] is False)
        write_registry(None)
        check("registry removed -> ready:false",
              client.post("/score", json={"items": ITEMS}).json()["ready"] is False)

    shutil.rmtree(tmp, ignore_errors=True)
    failed = [line for line in checks if line.startswith("FAIL")]
    print(f"\n{len(checks) - len(failed)}/{len(checks)} checks passed")
    for line in checks:
        print(line)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
