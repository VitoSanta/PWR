"""Reference solution used only to prove the acceptance suite is consistent.
Never copied into the workspace PWR works in."""

import csv
import io
import math
import re

ERRORS = {"#DIV/0!", "#VALUE!", "#NAME?", "#ERROR!", "#CYCLE!"}


class SheetError(Exception):
    def __init__(self, code):
        self.code = code


def col_to_num(col):
    n = 0
    for ch in col:
        n = n * 26 + (ord(ch) - 64)
    return n


def num_to_col(n):
    out = ""
    while n:
        n, r = divmod(n - 1, 26)
        out = chr(65 + r) + out
    return out


REF = re.compile(r"([A-Z]+)([0-9]+)$")


def split_ref(ref):
    m = REF.match(ref.upper())
    return col_to_num(m.group(1)), int(m.group(2))


TOKEN = re.compile(
    r"\s*(?:(?P<num>\d+(?:\.\d+)?)|(?P<str>\"[^\"]*\")|(?P<ref>[A-Za-z]+[0-9]+(?::[A-Za-z]+[0-9]+)?)"
    r"|(?P<name>[A-Za-z]+)|(?P<op><=|>=|<>|[-+*/^&=<>(),]))"
)


def tokenize(text):
    pos, out = 0, []
    text = text.rstrip()
    while pos < len(text):
        m = TOKEN.match(text, pos)
        if not m or m.end() == pos:
            raise SheetError("#ERROR!")
        pos = m.end()
        kind = m.lastgroup
        out.append((kind, m.group(kind)))
    return out


class Parser:
    def __init__(self, tokens):
        self.t, self.i = tokens, 0

    def peek(self):
        return self.t[self.i] if self.i < len(self.t) else (None, None)

    def take(self, value=None):
        tok = self.peek()
        if tok[0] is None or (value is not None and tok[1] != value):
            raise SheetError("#ERROR!")
        self.i += 1
        return tok

    def parse(self):
        node = self.comparison()
        if self.i != len(self.t):
            raise SheetError("#ERROR!")
        return node

    def comparison(self):
        node = self.concat()
        while self.peek()[1] in ("=", "<>", "<", ">", "<=", ">="):
            op = self.take()[1]
            node = ("bin", op, node, self.concat())
        return node

    def concat(self):
        node = self.additive()
        while self.peek()[1] == "&":
            self.take()
            node = ("bin", "&", node, self.additive())
        return node

    def additive(self):
        node = self.term()
        while self.peek()[1] in ("+", "-"):
            op = self.take()[1]
            node = ("bin", op, node, self.term())
        return node

    def term(self):
        node = self.power()
        while self.peek()[1] in ("*", "/"):
            op = self.take()[1]
            node = ("bin", op, node, self.power())
        return node

    def power(self):
        base = self.unary()
        if self.peek()[1] == "^":
            self.take()
            return ("bin", "^", base, self.power())
        return base

    def unary(self):
        if self.peek()[1] in ("-", "+"):
            op = self.take()[1]
            return ("neg", self.unary()) if op == "-" else self.unary()
        return self.primary()

    def primary(self):
        kind, value = self.peek()
        if kind == "num":
            self.take()
            return ("num", float(value) if "." in value else int(value))
        if kind == "str":
            self.take()
            return ("str", value[1:-1])
        if kind == "ref":
            self.take()
            return ("range", value.upper()) if ":" in value else ("ref", value.upper())
        if kind == "name":
            self.take()
            self.take("(")
            args = []
            if self.peek()[1] != ")":
                args.append(self.comparison())
                while self.peek()[1] == ",":
                    self.take()
                    args.append(self.comparison())
            self.take(")")
            return ("call", value.upper(), args)
        if value == "(":
            self.take()
            node = self.comparison()
            self.take(")")
            return node
        raise SheetError("#ERROR!")


def number(value):
    if isinstance(value, bool):
        return int(value)
    if value is None:
        return 0
    if isinstance(value, (int, float)):
        return value
    raise SheetError("#VALUE!")


def tidy(value):
    if isinstance(value, float) and value.is_integer():
        return int(value)
    return value


def text(value):
    if value is None:
        return ""
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    return str(tidy(value))


class Sheet:
    def __init__(self):
        self.raw = {}

    def set(self, ref, raw):
        ref = ref.upper()
        if raw == "":
            self.raw.pop(ref, None)
        else:
            self.raw[ref] = raw

    def get(self, ref):
        return self._value(ref.upper(), ())

    def _value(self, ref, stack):
        if ref in stack:
            raise_cycle = True
        else:
            raise_cycle = False
        if raise_cycle:
            return "#CYCLE!"
        raw = self.raw.get(ref)
        if raw is None:
            return None
        if not raw.startswith("="):
            try:
                return int(raw)
            except ValueError:
                try:
                    return float(raw)
                except ValueError:
                    return raw
        try:
            tree = Parser(tokenize(raw[1:])).parse()
            return tidy(self._eval(tree, stack + (ref,)))
        except SheetError as error:
            return error.code
        except ZeroDivisionError:
            return "#DIV/0!"

    def _cell(self, ref, stack):
        value = self._value(ref, stack)
        if isinstance(value, str) and value in ERRORS:
            raise SheetError(value)
        return value

    def _range(self, text_range, stack):
        a, b = text_range.split(":")
        c1, r1 = split_ref(a)
        c2, r2 = split_ref(b)
        return [
            self._cell(f"{num_to_col(c)}{r}", stack)
            for r in range(min(r1, r2), max(r1, r2) + 1)
            for c in range(min(c1, c2), max(c1, c2) + 1)
        ]

    def _eval(self, node, stack):
        kind = node[0]
        if kind in ("num", "str"):
            return node[1]
        if kind == "ref":
            return self._cell(node[1], stack)
        if kind == "range":
            raise SheetError("#VALUE!")
        if kind == "neg":
            return -number(self._eval(node[1], stack))
        if kind == "bin":
            _, op, left, right = node
            a, b = self._eval(left, stack), self._eval(right, stack)
            if op == "&":
                return text(a) + text(b)
            if op in ("=", "<>", "<", ">", "<=", ">="):
                if isinstance(a, str) or isinstance(b, str):
                    a, b = text(a), text(b)
                else:
                    a, b = number(a), number(b)
                return {"=": a == b, "<>": a != b, "<": a < b, ">": a > b, "<=": a <= b, ">=": a >= b}[op]
            a, b = number(a), number(b)
            if op == "+":
                return a + b
            if op == "-":
                return a - b
            if op == "*":
                return a * b
            if op == "/":
                if b == 0:
                    raise SheetError("#DIV/0!")
                return a / b
            return a ** b
        if kind == "call":
            _, name, args = node
            if name == "IF":
                if len(args) != 3:
                    raise SheetError("#ERROR!")
                return self._eval(args[1] if self._eval(args[0], stack) else args[2], stack)
            values = []
            for arg in args:
                if arg[0] == "range":
                    values.extend(v for v in self._range(arg[1], stack) if isinstance(v, (int, float)) and not isinstance(v, bool))
                else:
                    values.append(self._eval(arg, stack))
            nums = [number(v) for v in values] if name not in ("COUNT",) else values
            if name == "SUM":
                return sum(nums)
            if name == "AVERAGE":
                if not nums:
                    raise SheetError("#DIV/0!")
                return sum(nums) / len(nums)
            if name == "MIN":
                return min(nums) if nums else 0
            if name == "MAX":
                return max(nums) if nums else 0
            if name == "COUNT":
                return sum(1 for v in values if isinstance(v, (int, float)) and not isinstance(v, bool))
            if name == "ABS":
                return abs(nums[0])
            if name == "ROUND":
                return round(nums[0], int(nums[1]) if len(nums) > 1 else 0)
            raise SheetError("#NAME?")
        raise SheetError("#ERROR!")

    @classmethod
    def from_csv(cls, data):
        sheet = cls()
        for r, row in enumerate(csv.reader(io.StringIO(data)), start=1):
            for c, cell in enumerate(row, start=1):
                if cell != "":
                    sheet.set(f"{num_to_col(c)}{r}", cell)
        return sheet

    def to_csv(self):
        if not self.raw:
            return ""
        coords = [split_ref(ref) for ref in self.raw]
        cols = max(c for c, _ in coords)
        rows = max(r for _, r in coords)
        out = io.StringIO()
        writer = csv.writer(out, lineterminator="\n")
        for r in range(1, rows + 1):
            writer.writerow([text(self.get(f"{num_to_col(c)}{r}")) for c in range(1, cols + 1)])
        return out.getvalue()
