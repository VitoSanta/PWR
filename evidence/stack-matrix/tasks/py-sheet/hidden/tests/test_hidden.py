import unittest

from sheet import Sheet


class Hidden(unittest.TestCase):
    def test_long_dependency_chain_updates(self):
        s = Sheet()
        s.set("A1", "1")
        for row in range(2, 60):
            s.set(f"A{row}", f"=A{row - 1}+1")
        self.assertEqual(s.get("A59"), 59)
        s.set("A1", "100")
        self.assertEqual(s.get("A59"), 158)

    def test_power_is_right_associative_and_unary_minus_binds_tighter(self):
        s = Sheet()
        s.set("A1", "=2^3^2")
        s.set("A2", "=-2^2")
        self.assertEqual(s.get("A1"), 512)
        self.assertEqual(s.get("A2"), 4)

    def test_concatenation_writes_numbers_as_they_print(self):
        s = Sheet()
        s.set("A1", "1.5")
        s.set("B1", '="x"&A1&"-"&2')
        self.assertEqual(s.get("B1"), "x1.5-2")

    def test_count_skips_text_and_empty(self):
        s = Sheet()
        s.set("A1", "3")
        s.set("A2", "three")
        s.set("A4", "4.5")
        s.set("B1", "=COUNT(A1:A4)")
        self.assertEqual(s.get("B1"), 2)

    def test_error_inside_a_range_propagates(self):
        s = Sheet()
        s.set("A1", "=1/0")
        s.set("A2", "2")
        s.set("B1", "=SUM(A1:A2)")
        self.assertEqual(s.get("B1"), "#DIV/0!")
        s.set("A1", "1")
        self.assertEqual(s.get("B1"), 3)


if __name__ == "__main__":
    unittest.main()
