# Ad Center

`ad-center` owns advertiser campaigns, creative fields, delivery budget facts
and idempotent impression/click receipts. It is the control plane and budget
authority; serving stays in `ad-main` and its recall/rank dependencies.

Every campaign is bound to a published route action node and one declared scene
equipment label (`route_id`, `action_node_id` plus `scene_equipment`). Creation
revalidates the node and its equipment through BBS Link, and serving/decision
registration requires the exact tuple; standalone or cross-equipment placements
are rejected.

Campaigns may additionally declare `geo_regions` and `device_os` slugs
(normalized lower-case, e.g. `cn-bj`, `ios`). Both dimensions are hard
eligibility filters with a fail-closed contract: empty arrays mean
unrestricted, while restricted campaigns only serve requests whose delivery
context carries a matching value. Unknown or unobservable context never serves
targeted stock, so the gateway classifies what it can (today: the client user
agent) and passes absence through as an empty value. Eligibility compares exact
values backed by a GIN index; the request context travels through
`ad-main` → `ad-recall` → `Eligible`. No rank-side bonus is needed — every
ranked candidate already matches the context by construction.

Campaigns carry bounded `predicted_ctr` and `predicted_cvr` serving inputs for
the `ad-rank` eCPM auction. Frequency protection is enforced twice at receipt
time under the campaign row lock: `frequency_cap` limits a user/campaign/day and
`global_frequency_cap` limits a campaign/day across all users. A rejected
receipt never consumes campaign budget.

Eligibility pre-filtering uses Redis (`FrequencyGate`) as an accelerator, never
as an authority. Three day-scoped counter families — campaign×user
(`adfreq:`), campaign×global (`adgfreq:`) and the platform-wide per-user daily
total (`aduday:`) — are bumped atomically by one Lua script after each accepted
impression and compared during eligibility. Postgres re-adjudicates every
accepted impression at receipt time (including the cross-campaign daily total,
seeded by migration 0078), so gate counters may drift freely: a degraded or
absent Redis simply falls back to the fully SQL-adjudicated query, and deleting
a guardrail row can never silently disable the cap (readers fall back to the
documented default).

`bid_micros` is micro-currency per thousand impressions for CPM campaigns and
per click for CPC campaigns. CPM receipt charges are rounded against the
campaign's cumulative daily impression total so fractional micro-units are not
charged repeatedly.

Click receipts are accepted only after the same user has an accepted impression
for the same request and campaign. A click that arrives first is rejected and
does not consume CPC budget; retries remain idempotent after the impression is
recorded.

Its internal control plane supports create, update and campaign lookup. The
lookup response includes current-day spend, impressions and clicks, so an
operator can observe a campaign after it is activated without querying serving
stores directly.

## Delivery guardrails and advertiser reporting

The `ad_delivery_guardrails` table (migration 0078) holds the platform-wide
per-user daily total cap. It is exposed through three RPCs:

- `GetDeliveryGuardrails` — readable by advertisers (transparency) via the
  gateway's `GET /v1/admin/ads/guardrails`.
- `SetUserDailyTotalCap` — a platform safety control; the gateway route
  (`PATCH /v1/admin/ads/guardrails`) requires the platform `admin` role. A cap
  an advertiser could loosen would not be a guardrail.
- `DeliveryReport` — the daily ledger behind
  `GET /v1/admin/ads/reports?from&to` (max span one year, canonical
  `YYYY-MM-DD`). Rows aggregate `ad_campaign_daily_stats`, which is itself the
  durable projection of accepted delivery events written under receipt-time row
  locks (migration 0031), so report numbers match the adjudicated ledger by
  construction. Advertisers see only their own campaigns; conversions are
  intentionally absent until a conversion event source exists.

