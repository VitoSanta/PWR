WITH paid AS (
  SELECT customer_id, COUNT(*) AS orders FROM orders WHERE status = 'paid' GROUP BY customer_id
)
SELECT CASE WHEN COUNT(*) = 0 THEN 0.0 ELSE round(1.0 * SUM(orders >= 2) / COUNT(*), 3) END AS repeat_rate
FROM paid;
