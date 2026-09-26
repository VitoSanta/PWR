CREATE TABLE refunds (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id),
  amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
  reason TEXT NOT NULL CHECK (reason IN ('damaged', 'late', 'other')),
  created_at TEXT NOT NULL
);

CREATE INDEX idx_refunds_order ON refunds(order_id);
