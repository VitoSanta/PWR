import pathlib
import sqlite3
import unittest

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
QUERIES = ["revenue_by_month", "top_customers", "category_share", "repeat_customers", "product_activity"]


def database(seed=None):
    db = sqlite3.connect(":memory:")
    db.executescript((ROOT / "db/schema.sql").read_text())
    if seed:
        db.executescript(seed.read_text())
    return db


def result(db, path):
    cursor = db.execute(path.read_text())
    return [d[0] for d in cursor.description], cursor.fetchall()


class Hidden(unittest.TestCase):
    """The same reports on data the queries were not written against."""

    def check(self, seed):
        for name in QUERIES:
            with self.subTest(query=name):
                db = database(seed)
                self.assertEqual(
                    result(db, ROOT / "queries" / f"{name}.sql"),
                    result(db, HERE / "reference" / f"{name}.sql"),
                )

    def test_other_data(self):
        self.check(HERE / "other_seed.sql")

    def test_no_data(self):
        self.check(None)

    def test_products_but_no_sales(self):
        db = database()
        db.execute("INSERT INTO products VALUES (1, 'X', 'x', 'misc')")
        _, rows = result(db, ROOT / "queries" / "category_share.sql")
        self.assertEqual(rows, [("misc", 0, 0.0)])
        _, rows = result(db, ROOT / "queries" / "repeat_customers.sql")
        self.assertEqual(rows, [(0.0,)])


if __name__ == "__main__":
    unittest.main()
