#!/usr/bin/env python3
"""Builds suites/a3-editing.corpus.json, the tasks behind suite A3 (editing).

Each task is small, self-contained Python, and modelled on an edit that went
wrong in a recorded run. Each carries a reference fix, which never enters the
corpus, and a plausible wrong fix -- the failure it is modelled on. Before the
corpus is written every task is checked (see corpus_builder.py), so a task
that could be passed by the failure it exists to catch is never written. Run
from the repository root:

    python3 suites/build_a3_corpus.py
"""
from __future__ import annotations

import functools
import pathlib

from corpus_builder import build
from corpus_builder import task as _task

OUT = pathlib.Path(__file__).with_name("a3-editing.corpus.json")
task = functools.partial(_task, suite="A3")


TASKS = []

# 1. A regex in a raw string, edited near its backslash.
TIMEPARSE = '''import re

# Times look like 07:32:00 or 07:32:00.999999.
TIME_RE = re.compile(r"([01][0-9]|2[0-3]):([0-5][0-9]):([0-5][0-9])(?:\\.([0-9]{1,6}))?")


def parse_time(text):
    match = TIME_RE.fullmatch(text)
    if match is None:
        raise ValueError(f"not a time: {text!r}")
    hour, minute, second, fraction = match.groups()
    return (int(hour), int(minute), int(second), int((fraction or "0").ljust(6, "0")))
'''
TASKS.append(task(
    "a3-regex-backslash",
    "Seconds become optional in timeparse.py: parse_time('07:32') must return (7, 32, 0, 0). "
    "A fraction is still allowed only after seconds, as in '07:32:05.25'. The visible test fails. "
    "Change only timeparse.py.",
    ["timeparse.py"],
    {
        "timeparse.py": TIMEPARSE,
        "test_timeparse.py": '''import unittest

from timeparse import parse_time


class ParseTime(unittest.TestCase):
    def test_full(self):
        self.assertEqual(parse_time("07:32:00"), (7, 32, 0, 0))

    def test_fraction(self):
        self.assertEqual(parse_time("07:32:05.25"), (7, 32, 5, 250000))

    def test_without_seconds(self):
        self.assertEqual(parse_time("07:32"), (7, 32, 0, 0))
''',
    },
    '''from timeparse import parse_time

assert parse_time("07:32") == (7, 32, 0, 0)
assert parse_time("23:59:59") == (23, 59, 59, 0)
assert parse_time("07:32:05.25") == (7, 32, 5, 250000)
for bad in ["07:32:05x5", "07:32.5", "7:32", "24:00", "07:32:"]:
    try:
        parse_time(bad)
    except ValueError:
        continue
    raise AssertionError(f"{bad!r} was accepted")
print("ok")
''',
    {"timeparse.py": TIMEPARSE.replace(
        'r"([01][0-9]|2[0-3]):([0-5][0-9]):([0-5][0-9])(?:\\.([0-9]{1,6}))?"',
        'r"([01][0-9]|2[0-3]):([0-5][0-9])(?::([0-5][0-9])(?:\\.([0-9]{1,6}))?)?"',
    ).replace("int(second)", "int(second or 0)")},
    # The recorded failure: the backslash doubled inside the raw string.
    {"timeparse.py": TIMEPARSE.replace(
        'r"([01][0-9]|2[0-3]):([0-5][0-9]):([0-5][0-9])(?:\\.([0-9]{1,6}))?"',
        'r"([01][0-9]|2[0-3]):([0-5][0-9])(?::([0-5][0-9])(?:\\\\.([0-9]{1,6}))?)?"',
    ).replace("int(second)", "int(second or 0)")},
    "tomli-optional-seconds, Part E budget variant 2026-09-18 (a regex written with `\\\\.` for `\\.`)",
))

# 2. One method in a long file.
METRICS = "".join(
    f"\n    def metric_{i:03d}(self):\n        return sum(self._items) * {i}\n" for i in range(1, 181)
)
BAG = f'''class Bag:
    """A multiset of numbers."""

    def __init__(self, items=()):
        self._items = list(items)

    def __len__(self):
        return len(self._items)

    def __iter__(self):
        return iter(self._items)

    def __add__(self, other):
        if not isinstance(other, Bag):
            return NotImplemented
        return Bag(self._items + other._items)

    def __iadd__(self, other):
        self._items.extend(other._items)
        return self
{METRICS}'''
TASKS.append(task(
    "a3-one-method-long-file",
    "In bag.py, `bag + [1]` raises a clean TypeError but `bag += [1]` fails with an AttributeError "
    "about `_items`. `+=` with a non-Bag must raise TypeError the way `+` does, and `+=` with another "
    "Bag must keep working. The visible test fails. Change only the `__iadd__` method of bag.py.",
    ["bag.py"],
    {
        "bag.py": BAG,
        "test_bag.py": '''import unittest

from bag import Bag


class InPlaceAdd(unittest.TestCase):
    def test_bag(self):
        bag = Bag([1])
        bag += Bag([2])
        self.assertEqual(list(bag), [1, 2])

    def test_list_raises_type_error(self):
        bag = Bag([1])
        with self.assertRaises(TypeError):
            bag += [1]
''',
    },
    '''from bag import Bag

bag = Bag([1])
bag += Bag([2, 3])
assert list(bag) == [1, 2, 3]
for other in ([1], 1, "x"):
    try:
        b = Bag([1])
        b += other
    except TypeError:
        continue
    raise AssertionError(f"+= {other!r} did not raise TypeError")
missing = [i for i in range(1, 181) if not hasattr(Bag, f"metric_{i:03d}")]
assert not missing, f"methods lost: {missing[:5]}"
assert Bag([1, 2]).metric_180() == 540
print("ok")
''',
    {"bag.py": BAG.replace(
        "    def __iadd__(self, other):\n        self._items.extend",
        "    def __iadd__(self, other):\n        if not isinstance(other, Bag):\n            return NotImplemented\n        self._items.extend",
    )},
    # The recorded failure: the method sent as the whole file.
    {"bag.py": "class Bag:\n    def __iadd__(self, other):\n        if not isinstance(other, Bag):\n            return NotImplemented\n        self._items.extend(other._items)\n        return self\n"},
    "pyparsing-iadd-non-results, Part E with both fixes 2026-09-18 (one method sent as the whole of a 940-line file)",
))

# 3. A line full of quotes and braces.
GREET = '''def inbox_line(name, count):
    return f"Hello, {name}! You have {count} new message{'s' if count != 1 else ''}."
'''
TASKS.append(task(
    "a3-quotes-and-braces",
    "In greet.py, inbox_line('Ada', 0) must read \"Hello, Ada! You have no new messages.\" instead of "
    "\"0 new messages\". Other counts are unchanged. The visible test fails. Change only greet.py.",
    ["greet.py"],
    {
        "greet.py": GREET,
        "test_greet.py": '''import unittest

from greet import inbox_line


class InboxLine(unittest.TestCase):
    def test_none(self):
        self.assertEqual(inbox_line("Ada", 0), "Hello, Ada! You have no new messages.")

    def test_one(self):
        self.assertEqual(inbox_line("Ada", 1), "Hello, Ada! You have 1 new message.")

    def test_many(self):
        self.assertEqual(inbox_line("Ada", 3), "Hello, Ada! You have 3 new messages.")
''',
    },
    '''from greet import inbox_line

assert inbox_line("Ada", 0) == "Hello, Ada! You have no new messages."
assert inbox_line("Ada", 1) == "Hello, Ada! You have 1 new message."
assert inbox_line("Ada", 2) == "Hello, Ada! You have 2 new messages."
assert inbox_line('O"Brien {x}', 0) == 'Hello, O"Brien {x}! You have no new messages.'
assert inbox_line("{name}", 5) == "Hello, {name}! You have 5 new messages."
print("ok")
''',
    {"greet.py": '''def inbox_line(name, count):
    if count == 0:
        return f"Hello, {name}! You have no new messages."
    return f"Hello, {name}! You have {count} new message{'s' if count != 1 else ''}."
'''},
    # The new branch written without its `f` prefix: `{name}` reaches the user.
    {"greet.py": '''def inbox_line(name, count):
    if count == 0:
        return "Hello, {name}! You have no new messages."
    return f"Hello, {name}! You have {count} new message{'s' if count != 1 else ''}."
'''},
    "the escaping failures of 2026-09-18, where quotes and braces reached the model escaped",
))

# 4. A file indented with tabs.
SETTINGS = "def parse_line(line):\n\tline = line.strip()\n\tif not line or line.startswith(\"#\"):\n\t\treturn None\n\tkey, value = line.split(\"=\")\n\treturn key.strip(), value.strip()\n"
TASKS.append(task(
    "a3-tab-indented",
    "In settings.py, a value may itself contain '=': parse_line('url = a=b') must return ('url', 'a=b'). "
    "Split only at the first '='. The visible test fails. Change only settings.py and keep its tab "
    "indentation.",
    ["settings.py"],
    {
        "settings.py": SETTINGS,
        "test_settings.py": '''import unittest

from settings import parse_line


class ParseLine(unittest.TestCase):
    def test_simple(self):
        self.assertEqual(parse_line("name = PWR"), ("name", "PWR"))

    def test_comment(self):
        self.assertIsNone(parse_line("# note"))

    def test_value_with_equals(self):
        self.assertEqual(parse_line("url = a=b"), ("url", "a=b"))
''',
    },
    '''import pathlib

from settings import parse_line

assert parse_line("url = a=b=c") == ("url", "a=b=c")
assert parse_line("  k=v  ") == ("k", "v")
assert parse_line("") is None
for number, line in enumerate(pathlib.Path("settings.py").read_text().splitlines(), 1):
    body = line.lstrip("\\t")
    assert not body.startswith(" "), f"line {number} is indented with spaces"
print("ok")
''',
    {"settings.py": SETTINGS.replace('line.split("=")', 'line.split("=", 1)')},
    # The same fix with the body re-indented in spaces.
    {"settings.py": SETTINGS.replace("\t", "    ").replace('line.split("=")', 'line.split("=", 1)')},
    "whitespace failures of replace_text, where the find text's indentation did not match the file",
))

# 5. Two files in scope, a look-alike out of scope.
MONEY = '''def to_cents(amount):
    """'12.34' -> 1234."""
    return int(float(amount) * 100)
'''
INVOICE = '''from money import to_cents


def total_cents(lines):
    return sum(to_cents(price) * quantity for price, quantity in lines)


def tax_cents(total, rate_percent):
    return total * rate_percent // 100
'''
LEGACY = '''# Frozen: reproduces the 2019 reports byte for byte. Do not change.
def legacy_cents(amount):
    return int(float(amount) * 100)
'''
TASKS.append(task(
    "a3-two-files-and-a-lookalike",
    "Amounts lose a cent: to_cents('0.29') returns 28, and tax_cents truncates. to_cents in money.py "
    "must be exact, and tax_cents in invoice.py must round half up (22% of 25 cents is 6). The visible "
    "test fails. Change only money.py and invoice.py; legacy_report.py is frozen.",
    ["money.py", "invoice.py"],
    {
        "money.py": MONEY,
        "invoice.py": INVOICE,
        "legacy_report.py": LEGACY,
        "test_invoice.py": '''import unittest

from invoice import tax_cents, total_cents
from money import to_cents


class Invoice(unittest.TestCase):
    def test_exact_cents(self):
        self.assertEqual(to_cents("0.29"), 29)

    def test_total(self):
        self.assertEqual(total_cents([("0.29", 3), ("1.10", 1)]), 197)

    def test_tax_rounds_half_up(self):
        self.assertEqual(tax_cents(25, 22), 6)
''',
    },
    '''import pathlib

from invoice import tax_cents, total_cents
from money import to_cents

assert to_cents("0.29") == 29 and to_cents("19.99") == 1999 and to_cents("0.07") == 7
assert total_cents([("0.29", 3)]) == 87
assert tax_cents(87, 22) == 19 and tax_cents(25, 22) == 6 and tax_cents(100, 22) == 22
assert pathlib.Path("legacy_report.py").read_text() == LEGACY
print("ok")
'''.replace("== LEGACY", "== " + repr(LEGACY)),
    {
        "money.py": '''from decimal import Decimal


def to_cents(amount):
    """'12.34' -> 1234."""
    return int(Decimal(amount) * 100)
''',
        "invoice.py": INVOICE.replace("total * rate_percent // 100", "(total * rate_percent + 50) // 100"),
    },
    # Fixed everywhere, including the frozen look-alike.
    {
        "money.py": "def to_cents(amount):\n    return round(float(amount) * 100)\n",
        "invoice.py": INVOICE.replace("total * rate_percent // 100", "(total * rate_percent + 50) // 100"),
        "legacy_report.py": LEGACY.replace("int(float(amount) * 100)", "round(float(amount) * 100)"),
    },
    "out-of-scope edits checked in A.1 (Bionic's idna run rewrote files outside the allowed set)",
))

# 6. A file with Windows line endings.
UNITS = 'FACTORS = {\r\n    "km": 1000,\r\n    "m": 1,\r\n    "cm": 0.01,\r\n}\r\n\r\n\r\ndef to_meters(value, unit):\r\n    return value * FACTORS[unit]\r\n'
TASKS.append(task(
    "a3-crlf-line-endings",
    "units.py must also know millimetres ('mm' is 0.001 m), and an unknown unit must raise "
    "ValueError('unknown unit: <unit>') instead of KeyError. The visible test fails. Change only "
    "units.py and keep its Windows (CRLF) line endings.",
    ["units.py"],
    {
        "units.py": UNITS,
        "test_units.py": '''import unittest

from units import to_meters


class ToMeters(unittest.TestCase):
    def test_km(self):
        self.assertEqual(to_meters(2, "km"), 2000)

    def test_mm(self):
        self.assertAlmostEqual(to_meters(5, "mm"), 0.005)

    def test_unknown(self):
        with self.assertRaises(ValueError):
            to_meters(1, "parsec")
''',
    },
    '''import pathlib

from units import to_meters

assert abs(to_meters(1500, "mm") - 1.5) < 1e-9
try:
    to_meters(1, "ly")
except ValueError as error:
    assert "unknown unit: ly" in str(error), error
else:
    raise AssertionError("unknown unit accepted")
data = pathlib.Path("units.py").read_bytes()
assert data.count(b"\\n") == data.count(b"\\r\\n"), "line endings are no longer all CRLF"
print("ok")
''',
    {"units.py": UNITS.replace('    "cm": 0.01,\r\n', '    "cm": 0.01,\r\n    "mm": 0.001,\r\n').replace(
        "    return value * FACTORS[unit]\r\n",
        '    if unit not in FACTORS:\r\n        raise ValueError(f"unknown unit: {unit}")\r\n    return value * FACTORS[unit]\r\n',
    )},
    # The same fix with the new lines ending in LF.
    {"units.py": UNITS.replace('    "cm": 0.01,\r\n', '    "cm": 0.01,\n    "mm": 0.001,\n').replace(
        "    return value * FACTORS[unit]\r\n",
        '    if unit not in FACTORS:\n        raise ValueError(f"unknown unit: {unit}")\n    return value * FACTORS[unit]\r\n',
    )},
    "the product's Windows target: an edit must keep a file's line endings",
))


if __name__ == "__main__":
    build("a3-editing", TASKS, OUT)
