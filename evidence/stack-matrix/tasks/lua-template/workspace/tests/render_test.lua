local check = dofile("tests/check.lua")

check.render("plain text", "no tags here", {}, "no tags here")
check.render("a name", "Hello {{name}}!", { name = "Ada" }, "Hello Ada!")
check.render("spaces in the tag", "Hello {{ name }}!", { name = "Ada" }, "Hello Ada!")
check.render("missing is nothing", "[{{nope}}]", {}, "[]")
check.render("numbers", "{{n}} {{x}}", { n = 3, x = 2.5 }, "3 2.5")
check.render("booleans", "{{t}}|{{f}}", { t = true, f = false }, "true|")
check.render("escaped", "{{html}}", { html = [[<a href="x">Tom & 'Jerry'</a>]] },
  "&lt;a href=&quot;x&quot;&gt;Tom &amp; &#39;Jerry&#39;&lt;/a&gt;")
check.render("triple is raw", "{{{html}}}", { html = "<b>&</b>" }, "<b>&</b>")
check.render("ampersand is raw", "{{& html}}", { html = "<b>&</b>" }, "<b>&</b>")
check.render("comment", "a{{! ignore me }}b", {}, "ab")
check.render("dotted", "{{user.name.first}}", { user = { name = { first = "Grace" } } }, "Grace")
check.render("dotted missing", "[{{user.age.years}}]", { user = {} }, "[]")
check.render("newlines kept", "a\n{{x}}\nb\n", { x = "1" }, "a\n1\nb\n")

check.render("section over a list", "{{#items}}<{{name}}>{{/items}}",
  { items = { { name = "a" }, { name = "b" } } }, "<a><b>")
check.render("dot in a list", "{{#tags}}{{.}},{{/tags}}", { tags = { "x", "y", "z" } }, "x,y,z,")
check.render("section over a table", "{{#person}}{{name}} is {{age}}{{/person}}",
  { person = { name = "Linus", age = 30 } }, "Linus is 30")
check.render("truthy scalar", "{{#ok}}yes{{/ok}}", { ok = true }, "yes")
check.render("falsy section", "[{{#ok}}yes{{/ok}}]", { ok = false }, "[]")
check.render("empty list is falsy", "[{{#items}}x{{/items}}]", { items = {} }, "[]")
check.render("inverted", "{{^items}}none{{/items}}", { items = {} }, "none")
check.render("inverted when truthy", "[{{^ok}}no{{/ok}}]", { ok = true }, "[]")
check.render("outer names from inside", "{{#items}}{{greeting}} {{name}};{{/items}}",
  { greeting = "hi", items = { { name = "a" }, { name = "b" } } }, "hi a;hi b;")

check.raises("unclosed section", "{{#items}}x", { items = { 1 } }, "unclosed section 'items'")
check.raises("stray close", "x{{/items}}", {}, "unexpected closing tag 'items'")
check.raises("unclosed tag", "Hello {{name", { name = "x" }, "unclosed tag")

check.done()
