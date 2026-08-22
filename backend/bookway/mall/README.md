# Mall Catalog

`mall` owns product and SKU catalog facts, sale state and price snapshots. It
does not own stock reservations or order/payment state.

It also owns contextual `NodeOffer` records that attach an active SKU to a
public route action node. On every write and customer-facing read it directly
revalidates the node through BBS Link; the route must remain public, the node
must exist, and the offer creator must own that route. Each offer retains
creator attribution and a bounded commission rate, but never copies content or
a user's private action state.

The service-token-protected gRPC control plane creates products as drafts and
updates product fields, SKU price/attributes/saleability and lifecycle status.
Only active products and saleable SKUs are returned by customer-facing reads,
including the product projection nested in a node offer; merchant views may
include drafts and withdrawn SKUs. Order lines retain their own immutable price
snapshots.
