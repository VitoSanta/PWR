import pathlib
import sqlite3
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent


def database(seed="db/seed.sql"):
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    db.executescript((ROOT / "db/schema.sql").read_text())
    db.executescript((ROOT / seed).read_text())
    return db


def run(db, name):
    cursor = db.execute((ROOT / "queries" / f"{name}.sql").read_text())
    columns = [d[0] for d in cursor.description]
    return columns, cursor.fetchall()


class Reports(unittest.TestCase):
    def setUp(self):
        self.db = database()

    def test_revenue_by_month(self):
        columns, rows = run(self.db, "revenue_by_month")
        self.assertEqual(columns, ["month", "orders", "revenue_cents"])
        self.assertEqual(rows, [("2025-03", 3, 34394), ("2025-04", 2, 13994), ("2025-05", 2, 26899)])

    def test_top_customers(self):
        columns, rows = run(self.db, "top_customers")
        self.assertEqual(columns, ["customer", "country", "revenue_cents"])
        self.assertEqual(rows, [("Alba", "IT", 33894), ("Bruno", "IT", 26397), ("Chloé", "FR", 9997)])

    def test_category_share(self):
        columns, rows = run(self.db, "category_share")
        self.assertEqual(columns, ["category", "revenue_cents", "share"])
        self.assertEqual(rows, [
            ("displays", 44800, 58.3),
            ("peripherals", 25993, 33.9),
            ("accessories", 5994, 7.8),
        ])

    def test_repeat_customers(self):
        columns, rows = run(self.db, "repeat_customers")
        self.assertEqual(columns, ["repeat_rate"])
        self.assertEqual(rows, [(0.5,)])

    def test_product_activity(self):
        columns, rows = run(self.db, "product_activity")
        self.assertEqual(columns, ["sku", "first_sold", "last_sold", "units"])
        self.assertEqual(rows, [
            ("CB-USB", "2025-03-31", "2025-04-20", 6),
            ("KB-01", "2025-03-02", "2025-05-06", 4),
            ("MN-27", "2025-03-15", "2025-05-05", 2),
            ("MS-01", "2025-03-02", "2025-04-20", 3),
            ("ST-01", None, None, 0),
        ])


class Refunds(unittest.TestCase):
    def setUp(self):
        self.db = database()
        self.db.executescript((ROOT / "migrations/002_refunds.sql").read_text())

    def test_a_valid_refund_is_stored(self):
        self.db.execute("INSERT INTO refunds (order_id, amount_cents, reason, created_at) VALUES (1, 500, 'damaged', '2025-03-05')")
        self.assertEqual(self.db.execute("SELECT count(*) FROM refunds").fetchone()[0], 1)

    def test_constraints(self):
        bad = [
            "INSERT INTO refunds (order_id, amount_cents, reason, created_at) VALUES (1, 0, 'late', '2025-03-05')",
            "INSERT INTO refunds (order_id, amount_cents, reason, created_at) VALUES (1, 100, 'changed my mind', '2025-03-05')",
            "INSERT INTO refunds (order_id, amount_cents, reason, created_at) VALUES (99, 100, 'late', '2025-03-05')",
            "INSERT INTO refunds (order_id, amount_cents, reason) VALUES (1, 100, 'late')",
            "INSERT INTO refunds (amount_cents, reason, created_at) VALUES (100, 'late', '2025-03-05')",
        ]
        for statement in bad:
            with self.assertRaises(sqlite3.IntegrityError, msg=statement):
                self.db.execute(statement)

    def test_index(self):
        indexes = self.db.execute("SELECT name, tbl_name FROM sqlite_master WHERE type = 'index' AND name = 'idx_refunds_order'").fetchall()
        self.assertEqual(indexes, [("idx_refunds_order", "refunds")])
        columns = [row[2] for row in self.db.execute("PRAGMA index_info('idx_refunds_order')")]
        self.assertEqual(columns, ["order_id"])


if __name__ == "__main__":
    unittest.main()
