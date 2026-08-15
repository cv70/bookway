# Mall Catalog

`mall` owns product and SKU catalog facts, sale state and price snapshots. It
does not own stock reservations or order/payment state.

The service-token-protected gRPC control plane creates products as drafts and
updates product fields, SKU price/attributes/saleability and lifecycle status.
Only active products and saleable SKUs are returned by customer-facing reads;
order lines retain their own immutable price snapshots.
