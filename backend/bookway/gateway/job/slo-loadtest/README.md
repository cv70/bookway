# Gateway SLO Load Test

This finite release job measures the real Gateway Feed and Search HTTP paths against a running environment. It exits nonzero for any non-2xx response, transport failure, or P99 above the configured budget.

Run it only after deploying the complete Gateway dependency graph and seeding route, action-node, equipment, and search data:

```sh
GATEWAY_LOADTEST_URL=http://127.0.0.1:8080 \
GATEWAY_LOADTEST_BEARER_TOKEN='<signed-jwt>' \
GATEWAY_LOADTEST_REQUESTS=1000 \
GATEWAY_LOADTEST_CONCURRENCY=20 \
cargo run -p bookway-gateway-slo-loadtest
```

Configuration:

- `GATEWAY_LOADTEST_P99_MS`: P99 threshold, default `150`.
- `GATEWAY_LOADTEST_REQUEST_TIMEOUT_MS`: per-request client timeout, default `500`.
- `GATEWAY_LOADTEST_USER_ID`: evaluated member identity, default `slo-loadtest-user`.
- `GATEWAY_LOADTEST_BEARER_TOKEN`: optional Gateway Bearer JWT. Set this for environments with `AUTH_REQUIRED=true`; when omitted, the job uses `x-user-id` for local development only.
- `GATEWAY_LOADTEST_SEARCH_QUERY`: search query, default `跑步装备`.

The job emits one JSON report with Feed and Search latency percentiles. Keep that output with the release record required by `backend/deploy/SLO.md`.
