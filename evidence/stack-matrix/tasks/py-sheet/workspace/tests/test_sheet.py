"""Acceptance tests for the `sheet` package: a small spreadsheet engine.

Levels, easiest first. Each class is one level; the suite passes only when
every level does.
"""

import unittest

from sheet import Sheet


class L1Values(unittest.TestCase):
    def test_unset_cell_is_none(self):
        self.assertIsNone(Sheet().get("A1"))

    def test_integer_and_float_inputs(self):
        s = Sheet()
        s.set("A1", "42")
        s.set("A2", "2.5")
        s.set("A3", "-7")
        self.assertEqual(s.get("A1"), 42)
        self.assertIsInstance(s.get("A1"), int)
        self.assertEqual(s.get("A2"), 2.5)
        self.assertEqual(s.get("A3"), -7)

    def test_text_input_is_kept(self):
        s = Sheet()
        s.set("B2", "hello world")
        self.assertEqual(s.get("B2"), "hello world")

    def test_references_are_case_insensitive(self):
        s = Sheet()
        s.set("c3", "5")
        self.assertEqual(s.get("C3"), 5)

    def test_empty_string_clears_a_cell(self):
        s = Sheet()
        s.set("A1", "5")
        s.set("A1", "")
        self.assertIsNone(s.get("A1"))


class L2Arithmetic(unittest.TestCase):
    def check(self, formula, expected):
        s = Sheet()
        s.set("A1", formula)
        self.assertEqual(s.get("A1"), expected, formula)

    def test_operators_and_precedence(self):
        self.check("=1+2*3", 7)
        self.check("=(1+2)*3", 9)
        self.check("=2^3^2", 512)  # right-associative
        self.check("=-2^2", 4)  # unary minus binds tighter than ^
        self.check("=10/4", 2.5)
        self.check("=7-2-1", 4)

    def test_division_that_is_whole_stays_a_number(self):
        self.check("=8/2", 4)

    def test_whitespace_is_ignored(self):
        self.check("=  1 +   2 ", 3)

    def test_comparisons_return_booleans(self):
        self.check("=1<2", True)
        self.check("=2<=1", False)
        self.check("=3=3", True)
        self.check("=3<>3", False)

    def test_strings_and_concatenation(self):
        self.check('="ab"&"cd"', "abcd")
        self.check('="n="&5', "n=5")


class L3References(unittest.TestCase):
    def test_formula_reads_other_cells(self):
        s = Sheet()
        s.set("A1", "3")
        s.set("A2", "4")
        s.set("A3", "=A1*A1+A2*A2")
        self.assertEqual(s.get("A3"), 25)

    def test_empty_cell_counts_as_zero(self):
        s = Sheet()
        s.set("B1", "=A9+1")
        self.assertEqual(s.get("B1"), 1)

    def test_multi_letter_columns(self):
        s = Sheet()
        s.set("AA10", "2")
        s.set("AB10", "=AA10*10")
        self.assertEqual(s.get("AB10"), 20)

    def test_changes_propagate_to_dependents(self):
        s = Sheet()
        s.set("A1", "1")
        s.set("B1", "=A1+1")
        s.set("C1", "=B1*10")
        self.assertEqual(s.get("C1"), 20)
        s.set("A1", "5")
        self.assertEqual(s.get("B1"), 6)
        self.assertEqual(s.get("C1"), 60)

    def test_replacing_a_formula_with_a_value(self):
        s = Sheet()
        s.set("A1", "2")
        s.set("B1", "=A1*3")
        s.set("B1", "7")
        s.set("A1", "100")
        self.assertEqual(s.get("B1"), 7)


class L4Functions(unittest.TestCase):
    def setUp(self):
        self.s = Sheet()
        for i, value in enumerate(["4", "8", "15", "16", "23", "42"], start=1):
            self.s.set(f"A{i}", value)

    def test_sum_average_min_max_count_over_ranges(self):
        s = self.s
        s.set("B1", "=SUM(A1:A6)")
        s.set("B2", "=AVERAGE(A1:A4)")
        s.set("B3", "=MIN(A1:A6)")
        s.set("B4", "=MAX(A1:A6)")
        s.set("B5", "=COUNT(A1:A10)")
        self.assertEqual(s.get("B1"), 108)
        self.assertEqual(s.get("B2"), 10.75)
        self.assertEqual(s.get("B3"), 4)
        self.assertEqual(s.get("B4"), 42)
        self.assertEqual(s.get("B5"), 6)  # empty and text cells are not counted

    def test_functions_take_mixed_arguments_and_nest(self):
        s = self.s
        s.set("C1", "=SUM(A1, A2:A3, 100)")
        s.set("C2", "=MAX(SUM(A1:A2), 10)")
        self.assertEqual(s.get("C1"), 127)
        self.assertEqual(s.get("C2"), 12)

    def test_function_names_are_case_insensitive(self):
        self.s.set("D1", "=sum(a1:a2)")
        self.assertEqual(self.s.get("D1"), 12)

    def test_rectangular_ranges(self):
        s = Sheet()
        s.set("A1", "1")
        s.set("B1", "2")
        s.set("A2", "3")
        s.set("B2", "4")
        s.set("C1", "=SUM(A1:B2)")
        self.assertEqual(s.get("C1"), 10)

    def test_if_abs_round(self):
        s = self.s
        s.set("E1", '=IF(A1>5, "big", "small")')
        s.set("E2", '=IF(A6>5, "big", "small")')
        s.set("E3", "=ABS(A1-A6)")
        s.set("E4", "=ROUND(10/3, 2)")
        self.assertEqual(s.get("E1"), "small")
        self.assertEqual(s.get("E2"), "big")
        self.assertEqual(s.get("E3"), 38)
        self.assertEqual(s.get("E4"), 3.33)


class L5Errors(unittest.TestCase):
    def test_division_by_zero(self):
        s = Sheet()
        s.set("A1", "=1/0")
        self.assertEqual(s.get("A1"), "#DIV/0!")

    def test_text_in_arithmetic_is_a_value_error(self):
        s = Sheet()
        s.set("A1", "abc")
        s.set("A2", "=A1+1")
        self.assertEqual(s.get("A2"), "#VALUE!")

    def test_unknown_function(self):
        s = Sheet()
        s.set("A1", "=FOO(1)")
        self.assertEqual(s.get("A1"), "#NAME?")

    def test_malformed_formula(self):
        s = Sheet()
        for bad in ["=1+", "=(1+2", "=1 2", "=*3", "=SUM(1,", '="open']:
            s.set("A1", bad)
            self.assertEqual(s.get("A1"), "#ERROR!", bad)

    def test_errors_propagate_through_dependents(self):
        s = Sheet()
        s.set("A1", "=1/0")
        s.set("A2", "=A1+1")
        s.set("A3", "=SUM(A1:A2)")
        self.assertEqual(s.get("A2"), "#DIV/0!")
        self.assertEqual(s.get("A3"), "#DIV/0!")

    def test_error_clears_when_its_cause_is_fixed(self):
        s = Sheet()
        s.set("A1", "0")
        s.set("A2", "=10/A1")
        self.assertEqual(s.get("A2"), "#DIV/0!")
        s.set("A1", "4")
        self.assertEqual(s.get("A2"), 2.5)


class L6Cycles(unittest.TestCase):
    def test_self_reference(self):
        s = Sheet()
        s.set("A1", "=A1+1")
        self.assertEqual(s.get("A1"), "#CYCLE!")

    def test_every_cell_on_a_cycle_reports_it(self):
        s = Sheet()
        s.set("A1", "=B1")
        s.set("B1", "=C1")
        s.set("C1", "=A1")
        for ref in ("A1", "B1", "C1"):
            self.assertEqual(s.get(ref), "#CYCLE!", ref)

    def test_a_cell_depending_on_a_cycle_reports_it(self):
        s = Sheet()
        s.set("A1", "=B1")
        s.set("B1", "=A1")
        s.set("C1", "=A1*2")
        self.assertEqual(s.get("C1"), "#CYCLE!")

    def test_breaking_the_cycle_recovers(self):
        s = Sheet()
        s.set("A1", "=B1")
        s.set("B1", "=A1")
        s.set("B1", "7")
        self.assertEqual(s.get("A1"), 7)
        self.assertEqual(s.get("B1"), 7)

    def test_ranges_can_create_cycles(self):
        s = Sheet()
        s.set("A1", "1")
        s.set("A3", "=SUM(A1:A3)")
        self.assertEqual(s.get("A3"), "#CYCLE!")


class L7Csv(unittest.TestCase):
    def test_from_csv_places_cells_by_row_and_column(self):
        s = Sheet.from_csv("1,2,=A1+B1\nname,,=C1*2\n")
        self.assertEqual(s.get("A1"), 1)
        self.assertEqual(s.get("C1"), 3)
        self.assertEqual(s.get("A2"), "name")
        self.assertIsNone(s.get("B2"))
        self.assertEqual(s.get("C2"), 6)

    def test_to_csv_writes_values_over_the_used_rectangle(self):
        s = Sheet()
        s.set("A1", "1")
        s.set("B1", "=A1/4")
        s.set("C2", "=A1>0")
        s.set("B3", "=1/0")
        self.assertEqual(s.to_csv(), "1,0.25,\n,,TRUE\n,#DIV/0!,\n")

    def test_csv_quotes_fields_that_need_it(self):
        s = Sheet()
        s.set("A1", "a,b")
        s.set("B1", 'say "hi"')
        self.assertEqual(s.to_csv(), '"a,b","say ""hi"""\n')

    def test_round_trip_keeps_formulas_live(self):
        s = Sheet.from_csv("2,=A1*3\n")
        s.set("A1", "5")
        self.assertEqual(s.get("B1"), 15)


if __name__ == "__main__":
    unittest.main()
