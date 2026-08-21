# Ad Center

`ad-center` owns advertiser campaigns, creative fields, delivery budget facts
and idempotent impression/click receipts. It is the control plane and budget
authority; serving stays in `ad-main` and its recall/rank dependencies.

Every campaign is bound to a published route action node and one declared scene
equipment label (`route_id`, `action_node_id` plus `scene_equipment`). Creation
revalidates the node and its equipment through BBS Link, and serving/decision
registration requires the exact tuple; standalone or cross-equipment placements
are rejected.

Campaigns carry bounded `predicted_ctr` and `predicted_cvr` serving inputs for
the `ad-rank` eCPM auction. Frequency protection is enforced twice at receipt
time under the campaign row lock: `frequency_cap` limits a user/campaign/day and
`global_frequency_cap` limits a campaign/day across all users. A rejected
receipt never consumes campaign budget.

Click receipts are accepted only after the same user has an accepted impression
for the same request and campaign. A click that arrives first is rejected and
does not consume CPC budget; retries remain idempotent after the impression is
recorded.

Its internal control plane supports create, update and campaign lookup. The
lookup response includes current-day spend, impressions and clicks, so an
operator can observe a campaign after it is activated without querying serving
stores directly.
