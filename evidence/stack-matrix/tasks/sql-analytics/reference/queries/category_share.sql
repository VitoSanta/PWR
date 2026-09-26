WITH sold AS (
  SELECT p.category, SUM(i.quantity * i.unit_price_cents) AS revenue
  FROM order_items i
  JOIN orders o ON o.id = i.order_id AND o.status = 'paid'
  JOIN products p ON p.id = i.product_id
  GROUP BY p.category
),
total AS (SELECT COALESCE(SUM(revenue), 0) AS cents FROM sold)
SELECT c.category,
       COALESCE(s.revenue, 0) AS revenue_cents,
       CASE WHEN (SELECT cents FROM total) = 0 THEN 0.0
            ELSE round(100.0 * COALESCE(s.revenue, 0) / (SELECT cents FROM total), 1) END AS share
FROM (SELECT DISTINCT category FROM products) c
LEFT JOIN sold s ON s.category = c.category
ORDER BY revenue_cents DESC, c.category;
