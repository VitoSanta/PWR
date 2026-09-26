WITH order_revenue AS (
  SELECT o.id,
         substr(o.placed_at, 1, 7) AS month,
         (SELECT COALESCE(SUM(i.quantity * i.unit_price_cents), 0) FROM order_items i WHERE i.order_id = o.id)
           - o.discount_cents AS revenue
  FROM orders o
  WHERE o.status = 'paid'
)
SELECT month, COUNT(*) AS orders, SUM(revenue) AS revenue_cents
FROM order_revenue
GROUP BY month
ORDER BY month;
