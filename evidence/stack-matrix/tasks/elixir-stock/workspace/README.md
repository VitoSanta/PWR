# stock

Warehouse stock with reservations that expire, as an Elixir `GenServer`:
module `Stock` in `lib/stock.ex`. No dependencies.

```elixir
{:ok, pid} = Stock.start_link(now: fn -> System.monotonic_time(:millisecond) end)
{:ok, 10} = Stock.add(pid, "KB-01", 10)           # returns the new available count
{:ok, id} = Stock.reserve(pid, "KB-01", 3, 60_000) # ttl in milliseconds
7 = Stock.available(pid, "KB-01")
:ok = Stock.confirm(pid, id)                       # the 3 leave the warehouse
7 = Stock.on_hand(pid, "KB-01")
```

- `start_link(opts)`: `opts[:now]` is a zero-arity function returning the time
  in milliseconds (default: monotonic time); `opts[:name]` registers the
  process when given.
- `add(server, sku, qty)`: adds `qty` (a positive integer, else `{:error,
  :invalid_quantity}`) to `sku`'s stock on hand, creating the SKU if new;
  returns `{:ok, available}`.
- `reserve(server, sku, qty, ttl_ms)`: holds `qty` units for `ttl_ms`
  milliseconds. `{:ok, id}` (ids are unique), `{:error, :unknown_sku}`,
  `{:error, :invalid_quantity}` for a `qty` that is not a positive integer,
  or `{:error, :insufficient_stock}` when fewer than `qty` are available.
- `confirm(server, id)`: the reserved units leave the warehouse (on hand goes
  down). `:ok`, `{:error, :expired}` for a reservation whose time has
  passed, or `{:error, :not_found}` (unknown, cancelled or already
  confirmed).
- `cancel(server, id)`: releases the units. `:ok` or `{:error, :not_found}`
  (also for an expired one).
- `available(server, sku)`: on hand minus active reservations; `on_hand(server,
  sku)`: what is physically there. Both 0 for an unknown SKU.

A reservation is active until `now >= reserved_at + ttl_ms`; from then on it
holds nothing and cannot be confirmed. Expiry needs no timer: it is judged
from `now` whenever the server is asked.

Run the tests with `mix test`.
