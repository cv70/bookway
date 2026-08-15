# Mall Order Expirer

Runs `mall-order.ExpirePending` with the service token and continuously moves
expired pending-payment orders through inventory release to the terminal
`expired` state.

Start it after `mall-order`:

```bash
cargo run -p bookway-mall-order-expirer
```

Configure `MALL_ORDER_EXPIRER_BATCH_SIZE` (1-1000) and
`MALL_ORDER_EXPIRER_IDLE_MS` (100-60000) as needed.
