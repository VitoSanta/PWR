local check = dofile("tests/check.lua")

check.render("hidden: same name nested", "{{#a}}[{{#a}}x{{/a}}]{{/a}}", { a = { a = true } }, "[x]")
check.render("hidden: shadowing", "{{#p}}{{name}}{{/p}} {{name}}", { name = "outer", p = { name = "inner" } },
  "inner outer")
check.render("hidden: fall through a context without the name",
  "{{#items}}{{title}}:{{name}} {{/items}}", { title = "T", items = { { name = "a" }, { name = "b" } } },
  "T:a T:b ")
check.render("hidden: dotted names stop at the first context with the head",
  "{{#item}}[{{config.sep}}]{{/item}}", { config = { sep = "-" }, item = { config = {} } }, "[]")
check.render("hidden: dotted names from an outer context",
  "{{#item}}{{config.sep}}{{/item}}", { config = { sep = "-" }, item = { x = 1 } }, "-")
check.render("hidden: numbers in a list", "{{#nums}}{{.}}+{{/nums}}", { nums = { 1, 2, 3 } }, "1+2+3+")
check.render("hidden: dot is escaped", "{{#t}}{{.}}{{/t}}", { t = { "<" } }, "&lt;")
check.render("hidden: zero is truthy", "{{#zero}}yes{{/zero}}", { zero = 0 }, "yes")
check.render("hidden: the empty string is truthy", "{{#s}}yes{{/s}}", { s = "" }, "yes")
check.render("hidden: inverted over a missing name", "{{^missing}}none{{/missing}}", {}, "none")
check.render("hidden: inverted over false", "{{^f}}no{{/f}}", { f = false }, "no")
check.render("hidden: a float keeps its point", "{{f}}", { f = 3.0 }, "3.0")
check.render("hidden: newlines inside a section", "{{#l}}\n{{.}}{{/l}}", { l = { "a", "b" } }, "\na\nb")
check.render("hidden: unicode passes through", "{{x}}", { x = "città" }, "città")
check.render("hidden: a comment with a brace", "a{{! x } y }}b", {}, "ab")
check.render("hidden: spaces after the sigil", "{{# items }}{{ . }}{{/ items }}", { items = { "q" } }, "q")
check.render("hidden: raw with spaces", "{{& html }}|{{{ html }}}", { html = "<i>" }, "<i>|<i>")
check.render("hidden: a section over a nested list of tables",
  "{{#rows}}{{#cells}}{{v}}{{/cells}};{{/rows}}",
  { rows = { { cells = { { v = 1 }, { v = 2 } } }, { cells = { { v = 3 } } } } }, "12;3;")

check.raises("hidden: mismatched close", "{{#a}}{{/b}}", { a = true }, "unexpected closing tag 'b'")
check.raises("hidden: unclosed triple", "{{{name}}", { name = "x" }, "unclosed tag")
check.raises("hidden: outer section left open", "{{#a}}{{#b}}{{/b}}", { a = true, b = true },
  "unclosed section 'a'")

check.done()
