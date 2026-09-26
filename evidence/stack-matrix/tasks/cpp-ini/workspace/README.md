# ini

An INI configuration reader in C++17, standard library only, built with CMake.
The header is `include/ini/ini.hpp`, the implementation `src/ini.cpp`.

```cpp
namespace ini {

struct ParseError : std::runtime_error {
    int line;                              // 1-based line of the problem
    ParseError(int line, const std::string& message);
};

class Document {
public:
    static Document parse(std::string_view text);     // throws ParseError

    std::optional<std::string> get(std::string_view section, std::string_view key) const;
    std::string get_or(std::string_view section, std::string_view key, std::string_view fallback) const;
    std::optional<long long> get_int(std::string_view section, std::string_view key) const;
    std::optional<bool> get_bool(std::string_view section, std::string_view key) const;

    std::vector<std::string> sections() const;
    std::vector<std::string> keys(std::string_view section) const;
    std::string dump() const;
};

}
```

## Syntax

- Lines are `key = value`, `[section]` headers, comments or blank. Leading
  and trailing whitespace is ignored on every line, around keys and values,
  and inside the brackets of a header.
- A line whose first character is `;` or `#` is a comment. On a value line,
  ` ;` or ` #` (whitespace then the character) starts a comment that runs to
  the end of the line -- except inside quotes.
- Keys before the first header belong to the section `""`.
- A value in double quotes keeps its spaces and may contain `;` and `#`; in
  it `\"`, `\\` and `\n` are a quote, a backslash and a newline. Nothing but
  a comment may follow the closing quote.
- A key given twice in a section keeps the last value, in the place where it
  first appeared. A section given twice is one section.
- Section names and keys are case-sensitive.

`ParseError`, with its line, for: a line that is none of the above (no `=`),
an empty key, a header without its `]` or with an empty name, an unterminated
quote, anything but a comment after a closing quote.

## Reading

- `get` is the value, or no value when the section or key does not exist.
- `get_int` reads a whole decimal number with an optional sign; `get_bool`
  reads `true yes on 1` and `false no off 0` in any case. Both give no value
  for a missing key and throw `std::invalid_argument` for one that is there
  but is not of that type.
- `sections()` in order of first appearance, `""` first if it has keys;
  `keys(section)` in order of first appearance, empty for an unknown section.
- `dump()` writes the document back canonically: the `""` section's keys
  first, without a header, then each section as `[name]` followed by its keys,
  one `key = value` per line, a blank line between sections, `\n` endings. A
  value is quoted (with the escapes above) when it is empty, has spaces at
  either end, or contains `;`, `#`, `"`, `\` or a newline. Parsing the dump
  gives the same document.

Build and test with `sh scripts/test.sh` (configures `build/`, builds, runs `ctest`).
