#!/bin/sh
# Local/production launch. Model download (~2-3 GB for MiniCPM5-1B) happens
# on first start via modelscope snapshot_download and is cached.
set -e
cd "$(dirname "$0")"
exec "${PYTHON:-python3}" -m uvicorn app:app --host "${MODEL_SERVING_HOST:-127.0.0.1}" --port "${MODEL_SERVING_PORT:-8110}"
