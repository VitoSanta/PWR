INSERT INTO customers VALUES
  (1, 'Zed', 'US', '2024-01-01'),
  (2, 'Amy', 'US', '2024-01-02'),
  (3, 'Bob', 'UK', '2024-01-03'),
  (4, 'Cat', 'UK', '2024-01-04'),
  (5, 'Dan', 'US', '2024-01-05');
INSERT INTO products VALUES
  (1, 'A-1', 'Alpha', 'tools'),
  (2, 'B-1', 'Beta', 'tools'),
  (3, 'C-1', 'Gamma', 'garden'),
  (4, 'D-1', 'Delta', 'kitchen');
INSERT INTO orders VALUES
  (1, 1, '2024-12-31T23:00:00Z', 'paid', 0),
  (2, 2, '2025-01-01T00:30:00Z', 'paid', 0),
  (3, 3, '2025-01-15T10:00:00Z', 'paid', 100),
  (4, 4, '2025-02-01T10:00:00Z', 'cancelled', 0),
  (5, 1, '2025-02-02T10:00:00Z', 'paid', 0),
  (6, 5, '2025-03-01T10:00:00Z', 'pending', 0),
  (7, 5, '2025-03-02T10:00:00Z', 'cancelled', 0);
INSERT INTO order_items VALUES
  (1, 1, 2, 500),
  (2, 1, 1, 500),
  (2, 2, 1, 500),
  (3, 3, 4, 300),
  (3, 1, 1, 100),
  (4, 4, 10, 1000),
  (5, 2, 1, 1000),
  (6, 4, 1, 1000),
  (7, 3, 1, 50);
