"""MiniCPM5-1B model serving for 万卷行's recommendation stack.

x-algorithm keeps model inference behind dedicated services (media-model-proxy,
simclustersann); this service plays the same role for our LLM-based ranker:

* ``GET  /health``        — liveness + which checkpoint is loaded and from where.
* ``POST /v1/embeddings`` — OpenAI-compatible embeddings from the BASE model
  (mean-pooled last hidden state, LoRA adapters disabled). Usable the moment
  the base model is downloaded; no training required. Feeds semantic recall
  and RAG through knowledge-catalog's provider.
* ``POST /score``         — LLM ranking scores. Gated on a fine-tuned
  checkpoint produced by bookway-py/cronjob/rank_training/train_llm.py:
  the checkpoint's LoRA adapter is applied to the backbone and its scoring
  head is loaded — BOTH halves, a head without its adapter is a model
  mismatch and is refused. Without a valid checkpoint the endpoint reports
  ``ready: false`` and the Rust ranker keeps the heuristic; the service
  never invents scores.

Checkpoint resolution order per /score call (hot reload, no restart needed):

1. ``MODEL_REGISTRY_PATH`` — JSON registry published atomically by the
   training job when its holdout gate passes (auto-publish contract).
2. ``MODEL_CHECKPOINT_PATH`` — static operator-set checkpoint directory.

The model is treated as a BASE model: we read its hidden states, we do not
chat with it. trust_remote_code is required by OpenBMB model families.
"""

from __future__ import annotations

import json
import os
import threading
import time
from contextlib import asynccontextmanager
from typing import Any

import torch
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from transformers import AutoModel, AutoTokenizer

MODEL_NAME = os.environ.get("MODEL_NAME", "OpenBMB/MiniCPM5-1B")
MODEL_SOURCE = os.environ.get("MODEL_SOURCE", "modelscope")  # or "hf"
MODEL_DIR = os.environ.get("MODEL_DIR", "").strip()  # pre-downloaded snapshot
CHECKPOINT_PATH = os.environ.get("MODEL_CHECKPOINT_PATH", "").strip()
REGISTRY_PATH = os.environ.get("MODEL_REGISTRY_PATH", "").strip()
DEVICE = os.environ.get("MODEL_DEVICE", "cuda" if torch.cuda.is_available() else "cpu")
MAX_BATCH = int(os.environ.get("MODEL_MAX_BATCH", "64"))
MAX_INPUT_CHARS = int(os.environ.get("MODEL_MAX_INPUT_CHARS", "2048"))

app = FastAPI(title="bookway-model-serving", version="0.2.0")
_state: dict[str, Any] = {
    "tokenizer": None,
    "base_model": None,  # plain backbone; embeddings always run through this
    "hidden": None,
    "ready_since": None,
}
_scorer: dict[str, Any] = {
    "key": None,  # checkpoint dir + head mtime the current head+adapter came from
    "head": None,
    "model": None,  # backbone with the adapter active (single PeftModel, reused)
    "adapter_name": None,
    "adapter_seq": 0,
    "source": None,  # "registry" | "static"
    "loaded_since": None,
}
_load_lock = threading.Lock()
_infer_lock = threading.Lock()  # serialize forwards: one model copy, batch-serial


def _download() -> str:
    if MODEL_DIR:
        return MODEL_DIR
    if MODEL_SOURCE == "modelscope":
        from modelscope import snapshot_download

        return snapshot_download(MODEL_NAME)
    return MODEL_NAME


def _load() -> None:
    path = _download()
    tokenizer = AutoTokenizer.from_pretrained(path, trust_remote_code=True)
    model = AutoModel.from_pretrained(
        path,
        trust_remote_code=True,
        torch_dtype=torch.float32 if DEVICE == "cpu" else torch.float16,
    ).to(DEVICE)
    model.eval()
    _state["tokenizer"] = tokenizer
    _state["base_model"] = model
    _state["hidden"] = int(model.config.hidden_size)
    _state["ready_since"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def _valid_checkpoint(path: str) -> bool:
    """A checkpoint is servable only when BOTH halves of the contract exist.

    train_llm.py writes ``scoring_head.pt`` + ``adapter/``; the head was
    trained on adapter-tuned hidden states, so loading it over the raw base
    would be a silent model mismatch — refused, not approximated.
    """
    return os.path.isfile(os.path.join(path, "scoring_head.pt")) and os.path.isdir(
        os.path.join(path, "adapter")
    )


def _resolve_checkpoint() -> tuple[str, str] | None:
    """(checkpoint_dir, source) — registry first, then the static path."""
    if REGISTRY_PATH:
        try:
            with open(REGISTRY_PATH, encoding="utf-8") as handle:
                entry = json.load(handle)
            checkpoint = str(entry.get("checkpoint_path", "")).strip()
            if checkpoint and _valid_checkpoint(checkpoint):
                return checkpoint, "registry"
        except (OSError, ValueError):
            pass  # registry absent/corrupt: fall through, never guess
    if CHECKPOINT_PATH and _valid_checkpoint(CHECKPOINT_PATH):
        return CHECKPOINT_PATH, "static"
    return None


def _ensure_scorer() -> dict[str, Any] | None:
    """Load (or hot-reload) the scoring head + LoRA adapter, then cache.

    Called on every /score request: a registry stat + dict compare is the
    whole steady-state cost, so a freshly published model goes live without
    a restart. Loading happens under a lock exactly once per checkpoint.

    The backbone is wrapped in a PeftModel ONCE; reloads swap named adapters
    (load_adapter/set_adapter/delete_adapter) instead of re-wrapping, which
    peft does not support on an already-adapted module tree. The cache key
    includes the head file's mtime so a re-trained artifact at the same
    path reloads too.
    """
    if _state["base_model"] is None:
        return None
    resolved = _resolve_checkpoint()
    if resolved is None:
        return None
    checkpoint, source = resolved
    head_stat = os.stat(os.path.join(checkpoint, "scoring_head.pt"))
    key = f"{checkpoint}:{int(head_stat.st_mtime)}"
    if _scorer["key"] == key and _scorer["head"] is not None:
        return _scorer
    with _load_lock:
        if _scorer["key"] == key and _scorer["head"] is not None:
            return _scorer
        from checkpoint import load_scorer  # local module; trained by train_llm.py
        from peft import PeftModel

        head = load_scorer(checkpoint, int(_state["hidden"]), DEVICE)
        adapter_dir = os.path.join(checkpoint, "adapter")
        seq = int(_scorer["adapter_seq"]) + 1
        name = f"adapter-{seq}"
        current = _scorer["model"]
        if current is None:
            model = PeftModel.from_pretrained(
                _state["base_model"], adapter_dir, adapter_name=name
            )
        else:
            current.load_adapter(adapter_dir, adapter_name=name)
            current.set_adapter(name)
            current.delete_adapter(_scorer["adapter_name"])
            model = current
        model.eval()
        _scorer.update(
            key=key,
            head=head,
            model=model,
            adapter_name=name,
            adapter_seq=seq,
            source=source,
            loaded_since=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        )
        print(f"scorer loaded from {checkpoint} ({source})", flush=True)
        return _scorer


class EmbeddingRequest(BaseModel):
    input: str | list[str]
    model: str | None = None  # accepted for OpenAI compatibility; base model is fixed


class ScoreRequest(BaseModel):
    """Batch scoring request from recommend-rank.

    Each item carries the user context text and one candidate's text; the
    checkpoint is a three-objective scoring head over the LoRA-adapted
    backbone.
    """

    items: list[dict]


def _mean_pool(last_hidden: torch.Tensor, attention_mask: torch.Tensor) -> torch.Tensor:
    mask = attention_mask.unsqueeze(-1).to(last_hidden.dtype)
    summed = (last_hidden * mask).sum(dim=1)
    counts = mask.sum(dim=1).clamp(min=1e-9)
    return summed / counts


@asynccontextmanager
async def lifespan(_: FastAPI):
    try:
        _load()
    except Exception as error:  # noqa: BLE001 — startup must report, not crash-loop silently
        print(f"model load failed: {error}", flush=True)
        raise
    yield


app.router.lifespan_context = lifespan


def _checkpoint_view() -> dict[str, Any]:
    resolved = _resolve_checkpoint()
    view: dict[str, Any] = {
        "configured": {
            "checkpoint_path": CHECKPOINT_PATH or None,
            "registry_path": REGISTRY_PATH or None,
        },
        "resolved": None,
    }
    if _scorer["key"] is not None:
        view["loaded"] = {
            "checkpoint": _scorer["key"].rsplit(":", 1)[0],
            "source": _scorer["source"],
            "loaded_since": _scorer["loaded_since"],
        }
    if resolved is not None:
        view["resolved"] = {"checkpoint": resolved[0], "source": resolved[1]}
    return view


@app.get("/health")
def health() -> dict:
    base_loaded = _state["base_model"] is not None
    return {
        "status": "ok" if base_loaded else "loading",
        "model": MODEL_NAME,
        "source": MODEL_SOURCE,
        "hidden_size": _state["hidden"],
        "scorer": _checkpoint_view(),
        "scorer_ready": _scorer["head"] is not None,
        "ready_since": _state["ready_since"],
    }


@app.post("/v1/embeddings")
def embeddings(request: EmbeddingRequest) -> dict:
    texts = [request.input] if isinstance(request.input, str) else request.input
    if not texts or len(texts) > MAX_BATCH:
        raise HTTPException(status_code=400, detail=f"input must hold 1..{MAX_BATCH} texts")
    texts = [text[:MAX_INPUT_CHARS] for text in texts]
    tokenizer, model = _state["tokenizer"], _state["base_model"]
    if model is None:
        raise HTTPException(status_code=503, detail="model still loading")
    started = time.monotonic()
    encoded = tokenizer(texts, padding=True, truncation=True, return_tensors="pt").to(DEVICE)
    with _infer_lock, torch.no_grad():
        # Serving-scoped adapters: disabled here so semantic vectors stay on
        # the base model the indexer was dimensioned/calibrated against.
        active = _scorer["model"]
        if active is None:
            output = model(**encoded, return_dict=True)
        else:
            with active.disable_adapter():
                output = model(**encoded, return_dict=True)
    last_hidden = getattr(output, "last_hidden_state", None)
    if last_hidden is None:
        # Some causal LMs only expose decoder outputs; the final hidden state
        # is the last layer's hidden states either way.
        last_hidden = output[0]
    vectors = _mean_pool(last_hidden.float(), encoded["attention_mask"])
    data = [
        {"object": "embedding", "index": index, "embedding": vector.tolist()}
        for index, vector in enumerate(vectors)
    ]
    return {
        "object": "list",
        "data": data,
        "model": MODEL_NAME,
        "usage": {
            "prompt_tokens": int(encoded["attention_mask"].sum()),
            "total_ms": int((time.monotonic() - started) * 1000),
        },
    }


@app.post("/score")
def score(request: ScoreRequest) -> dict:
    if not request.items or len(request.items) > MAX_BATCH:
        raise HTTPException(status_code=400, detail=f"items must hold 1..{MAX_BATCH} entries")
    scorer = _ensure_scorer()
    if scorer is None:
        # Honest contract: no trained scorer, no invented scores. recommend-rank
        # treats this as degraded and keeps the heuristic ranking.
        return {"ready": False, "model_version": "minicpm-base-uncalibrated", "scores": []}

    tokenizer = _state["tokenizer"]
    head, model = scorer["head"], scorer["model"]
    texts = [
        f"{item.get('user_context', '')}\n[SEP]\n{item.get('candidate_text', '')}"[:MAX_INPUT_CHARS]
        for item in request.items
    ]
    encoded = tokenizer(texts, padding=True, truncation=True, return_tensors="pt").to(DEVICE)
    with _infer_lock, torch.no_grad():
        output = model(**encoded, return_dict=True)
        pooled = _mean_pool(output[0].float(), encoded["attention_mask"])
        logits = head(pooled)
    scores = torch.sigmoid(logits).tolist()
    return {
        "ready": True,
        "model_version": f"minicpm-scorer-{os.path.basename(scorer['key'].rsplit(':', 1)[0])}",
        "scores": [{"p_ctr": row[0], "p_cvr": row[1], "p_wegu": row[2]} for row in scores],
    }
