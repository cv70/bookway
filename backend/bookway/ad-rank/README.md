# Ad Rank

`ad-rank` produces a deterministic, versioned initial ranking. Its scoring
contract is isolated so an online model can replace the heuristic safely.

## Scoring versions

- `ecpm-v2` — static contract: `bid * pCTR * pCVR` using exactly the rates the
  advertiser declared at creation time.
- `ecpm-v3` (default) — calibrated CTR: the serving input is the Beta(1, 1)
  posterior mean over lifetime delivery evidence,
  `(clicks + 1) / (impressions + 2)`, blended evenly with the declared rate
  and clamped into `[declared / 2, declared * 2]`. Campaigns without any
  observed impression keep their declared rate unchanged, and `predicted_cvr`
  stays declared because the event pipeline records no conversion source yet.

Set `AD_RANK_CALIBRATION=false` for a one-key rollback to the exact static
`ecpm-v2` inputs; `AD_RANK_MODEL_VERSION` only renames the version string in
responses, it does not change scoring.
