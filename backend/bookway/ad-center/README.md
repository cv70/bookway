# Ad Center

`ad-center` owns advertiser campaigns, creative fields, delivery budget facts
and idempotent impression/click receipts. It is the control plane and budget
authority; serving stays in `ad-main` and its recall/rank dependencies.

Its internal control plane supports create, update and campaign lookup. The
lookup response includes current-day spend, impressions and clicks, so an
operator can observe a campaign after it is activated without querying serving
stores directly.
