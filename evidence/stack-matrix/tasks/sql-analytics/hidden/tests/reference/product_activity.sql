SELECT p.sku,
       MIN(substr(o.placed_at, 1, 10)) AS first_sold,
       MAX(substr(o.placed_at, 1, 10)) AS last_sold,
       COALESCE(SUM(i.quantity), 0) AS units
FROM products p
LEFT JOIN order_items i
  ON i.product_id = p.id AND i.order_id IN (SELECT id FROM orders WHERE status = 'paid')
LEFT JOIN orders o ON o.id = i.order_id
GROUP BY p.id
ORDER BY p.sku;
