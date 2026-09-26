INSERT INTO customers VALUES
  (1, 'Alba', 'IT', '2025-01-10'),
  (2, 'Bruno', 'IT', '2025-02-01'),
  (3, 'Chloé', 'FR', '2025-02-15'),
  (4, 'Dieter', 'DE', '2025-03-01'),
  (5, 'Eva', 'DE', '2025-03-20');

INSERT INTO products VALUES
  (1, 'KB-01', 'Keyboard', 'peripherals'),
  (2, 'MS-01', 'Mouse', 'peripherals'),
  (3, 'MN-27', 'Monitor 27"', 'displays'),
  (4, 'CB-USB', 'USB-C cable', 'accessories'),
  (5, 'ST-01', 'Stand', 'accessories');

INSERT INTO orders VALUES
  (1, 1, '2025-03-02T10:00:00Z', 'paid', 0),
  (2, 2, '2025-03-15T12:30:00Z', 'paid', 500),
  (3, 1, '2025-03-31T23:59:59Z', 'paid', 0),
  (4, 3, '2025-04-01T00:00:00Z', 'cancelled', 0),
  (5, 3, '2025-04-03T09:00:00Z', 'paid', 1000),
  (6, 4, '2025-04-10T15:00:00Z', 'pending', 0),
  (7, 2, '2025-04-20T18:00:00Z', 'paid', 0),
  (8, 1, '2025-05-05T08:00:00Z', 'paid', 0),
  (9, 4, '2025-05-06T08:00:00Z', 'paid', 0);

INSERT INTO order_items VALUES
  (1, 1, 1, 4999),
  (1, 2, 2, 1999),
  (2, 3, 1, 22900),
  (3, 4, 3, 999),
  (4, 3, 2, 22900),
  (5, 1, 2, 4999),
  (5, 4, 1, 999),
  (6, 2, 1, 1999),
  (7, 2, 1, 1999),
  (7, 4, 2, 999),
  (8, 3, 1, 21900),
  (9, 1, 1, 4999);
