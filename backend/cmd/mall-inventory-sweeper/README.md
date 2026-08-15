# Mall Inventory Sweeper

Runs `mall-inventory.ExpireReservations` with the service token and frees
inventory held by abandoned reservations. It is deliberately separate from the
order expirer so other future reservation users receive the same TTL guarantee.

Start it after `mall-inventory`:

```bash
cargo run -p bookway-mall-inventory-sweeper
```

Configure `MALL_INVENTORY_SWEEPER_BATCH_SIZE` (1-1000) and
`MALL_INVENTORY_SWEEPER_IDLE_MS` (100-60000) as needed.
