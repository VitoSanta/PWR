# template

A small Mustache-style template renderer in Lua 5.4, in `src/template.lua`.
`sh tests/run.sh` runs every `tests/*_test.lua` file with `lua`; the tests
are the contract. Only the Lua standard library may be used.

## API

```lua
local template = require("template")
local text = template.render(source, view)   -- source: string, view: table
```

`render` returns a string, or raises an error (with `error`) for a malformed
template.

## Tags

| Tag | Meaning |
|---|---|
| `{{name}}` | the value of `name`, HTML-escaped |
| `{{{name}}}` or `{{& name}}` | the value of `name`, as it is |
| `{{#name}}...{{/name}}` | a section (below) |
| `{{^name}}...{{/name}}` | an inverted section (below) |
| `{{! anything }}` | a comment: renders as nothing |
| `{{.}}` | the current context itself (the item, inside a section over a list) |

Spaces inside a tag around the name are ignored: `{{ name }}` is `{{name}}`,
`{{# items }}` is `{{#items}}`. A tag is replaced exactly where it stands;
the text around it, newlines included, is kept as it is.

Escaping replaces `&`, `<`, `>`, `"` and `'` with `&amp;`, `&lt;`, `&gt;`,
`&quot;` and `&#39;`.

## Values

A value is written as `tostring` writes it (`3`, `2.5`, `true`), except that
`nil` and `false` are written as nothing. Falsy, for sections, means `nil`,
`false`, or an empty table; everything else is truthy, including `0` and the
empty string (as in Lua).

## Names and the context stack

Rendering keeps a stack of contexts; the view is at the bottom. A name is
looked up from the top of the stack down, in the first context that is a
table and has a non-nil value for it. A dotted name `a.b.c` looks up `a` that
way, then takes `.b` and `.c` from what it found; if any step is missing or
not a table, the value is `nil`. `.` is the top of the stack.

## Sections

`{{#name}}block{{/name}}` renders `block`:

- once per element when the value is a list (a table with `value[1] ~= nil`),
  each element pushed on the stack while its copy renders;
- once, with the value pushed, when it is any other truthy value;
- not at all when it is falsy.

`{{^name}}block{{/name}}` renders `block` once, with nothing pushed, exactly
when the value is falsy.

Sections nest, including a section inside another of the same name.

## Errors

- A section left open: `unclosed section 'name'`.
- A closing tag that does not close the innermost open section:
  `unexpected closing tag 'name'`.
- `{{` with no `}}` after it (or `{{{` with no `}}}`): `unclosed tag`.

The message raised contains that text (Lua may prefix a position).
