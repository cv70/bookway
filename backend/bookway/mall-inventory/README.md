# Mall Inventory

`mall-inventory` owns available stock and reservation state. Reservations are
idempotent by order ID, time-bound, and atomically confirmed or released.

`ExpireReservations` is an internal, bounded sweep endpoint. Run
`bookway-mall-inventory-sweeper` in production so abandoned reservations free
stock even when no later read or checkout request arrives.
