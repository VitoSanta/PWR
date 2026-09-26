WITH order_revenue AS (
  SELECT o.customer_id,
         (SELECT COALESCE(SUM(i.quantity * i.unit_price_cents), 0) FROM order_items i WHERE i.order_id = o.id)
           - o.discount_cents AS revenue
  FROM orders o
  WHERE o.status = 'paid'
)
SELECT c.name AS customer, c.country, SUM(r.revenue) AS revenue_cents
FROM order_revenue r
JOIN customers c ON c.id = r.customer_id
GROUP BY c.id
ORDER BY revenue_cents DESC, c.name
LIMIT 3;
