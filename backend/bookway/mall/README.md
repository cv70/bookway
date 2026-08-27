# Mall Catalog

`mall` owns product and SKU catalog facts, sale state and price snapshots. It
does not own stock reservations or order/payment state.

It also owns contextual `NodeOffer` records that attach an active SKU to a
public route action node and one declared `scene_equipment`. On every write and
customer-facing read it directly revalidates the route, node and equipment
through BBS Link; public offer reads require an equipment context and never
return offers from another equipment context on the same node. Each offer
retains creator attribution and a bounded commission rate, but never copies
content or a user's private action state.

The service-token-protected gRPC control plane creates products as drafts and
updates product fields, SKU price/attributes/saleability and lifecycle status.
Only active products and saleable SKUs are returned by customer-facing reads,
including the product projection nested in a node offer; merchant views may
include drafts and withdrawn SKUs. Order lines retain their own immutable price
snapshots.

## Catalogue kinds and knowledge products

Every product carries a `product_kind` — `physical`, `course` or
`resource_pack` (migration 0076). Knowledge products bind a public
knowledge-catalog resource through `course_resource_id`; physical goods never
carry one. Binding rules are enforced fail-closed at write time against
`KnowledgeCatalog.Get`: courses must reference a *published* resource of kind
*course*, resource packs any published resource, and the mall rejects the write
whenever the catalog cannot confirm the binding (missing, unpublished or
unreachable). Updates must state the complete `(product_kind,
course_resource_id)` pair so validation always judges the exact values that
will persist. Node offers attach to knowledge-product SKUs through the same
ownership/saleability chain as physical goods; checkout keeps validating only
the route action context and SKU state, so an archived resource blocks new
bindings but does not retroactively rewrite offers.

