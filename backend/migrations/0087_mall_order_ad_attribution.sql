-- Server-side ad conversion attribution. An order that arrived through a
-- served ad creative carries the ad decision context (request id + campaign
-- id) captured at checkout; when the order is PAID the purchase outbox row
-- carries the same context, and cmd/outbox-relay reports the conversion to
-- ad-center. Clients can never assert a conversion themselves: the public
-- ad-event beacon path rejects the conversion type, and this fact is born
-- exclusively from the server-verified payment.
ALTER TABLE mall_orders ADD COLUMN ad_request_id TEXT;
ALTER TABLE mall_orders ADD COLUMN ad_campaign_id TEXT;
ALTER TABLE purchase_event_outbox ADD COLUMN ad_request_id TEXT;
ALTER TABLE purchase_event_outbox ADD COLUMN ad_campaign_id TEXT;
