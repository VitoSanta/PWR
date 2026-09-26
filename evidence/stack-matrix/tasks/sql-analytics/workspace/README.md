# shop analytics

The shop's reporting queries, in SQLite SQL. The schema is `db/schema.sql`,
a sample of real data `db/seed.sql`. Each report is one file in `queries/`
holding a single `SELECT` (a `WITH` in front is fine) that returns exactly
the columns named below, in that order and sorted as stated. The queries must
work on any data in this schema, not just the sample.

Money is integer cents. An order's **revenue** is the sum of its items'
`quantity * unit_price_cents`, minus its `discount_cents`. Only **paid**
orders count as revenue, in every report.

| File | Columns | Rows |
|---|---|---|
| `queries/revenue_by_month.sql` | `month` (`YYYY-MM` of `placed_at`), `orders`, `revenue_cents` | one per month with at least one paid order, by month |
| `queries/top_customers.sql` | `customer`, `country`, `revenue_cents` | the 3 customers with the most revenue, most first, ties by name; customers with no paid order never appear |
| `queries/category_share.sql` | `category`, `revenue_cents`, `share` | every category of `products` (0 for one never sold), by revenue descending then category; `share` is the category's percentage of item revenue (items only, discounts ignored), rounded to 1 decimal, 0.0 when there is none |
| `queries/repeat_customers.sql` | `repeat_rate` | one row: customers with at least two paid orders divided by customers with at least one, rounded to 3 decimals; 0.0 when no one has paid |
| `queries/product_activity.sql` | `sku`, `first_sold`, `last_sold`, `units` | every product by `sku`: the dates (`YYYY-MM-DD`) of its first and last paid order and the units sold in paid orders; `NULL`, `NULL`, `0` for a product never sold |

And one migration, `migrations/002_refunds.sql`, run after the schema: a
table `refunds` with `id INTEGER PRIMARY KEY`, `order_id` (required, a
reference to `orders`), `amount_cents` (required, greater than 0),
`reason` (required, one of `damaged`, `late`, `other`) and `created_at`
(required text); plus an index named `idx_refunds_order` on `order_id`.

Run the tests with `python3 -m unittest discover -s tests -v`.
