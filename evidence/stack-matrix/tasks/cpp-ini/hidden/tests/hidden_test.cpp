#include <stdexcept>

#include "check.hpp"
#include "ini/ini.hpp"

int main() {
  // A quote and a backslash inside a value, round trip.
  auto doc = ini::Document::parse("[s]\nv = \"a\\\\b \\\"c\\\"\"\nhash = \"#1\"\n");
  CHECK_EQ(doc.get("s", "v").value_or("?"), std::string("a\\b \"c\""));
  auto again = ini::Document::parse(doc.dump());
  CHECK_EQ(again.get("s", "v").value_or("?"), std::string("a\\b \"c\""));
  CHECK_EQ(again.get("s", "hash").value_or("?"), std::string("#1"));

  // A ; or # without whitespace before it is part of the value.
  auto inline_marks = ini::Document::parse("url = http://x/#top\nratio = 3;4\n");
  CHECK_EQ(inline_marks.get("", "url").value_or("?"), std::string("http://x/#top"));
  CHECK_EQ(inline_marks.get("", "ratio").value_or("?"), std::string("3;4"));

  // A comment after a closing quote is fine.
  auto commented = ini::Document::parse("k = \"v\" ; note\n");
  CHECK_EQ(commented.get("", "k").value_or("?"), std::string("v"));

  // Values may contain '='.
  auto eq = ini::Document::parse("expr = a=b\n");
  CHECK_EQ(eq.get("", "expr").value_or("?"), std::string("a=b"));

  // Windows line endings are whitespace at the end of a line.
  auto crlf = ini::Document::parse("[s]\r\nk = v\r\n");
  CHECK_EQ(crlf.get("s", "k").value_or("?"), std::string("v"));

  // An empty section still counts as a section.
  CHECK(ini::Document::parse("[a]\n[b]\nk = v\n").sections() == (std::vector<std::string>{"a", "b"}));

  // Errors count lines from 1, blank and comment lines included.
  CHECK_THROWS_LINE(ini::Document::parse("; c\n\n[ok]\nk = v\n\n??\n"), 6);
  return REPORT();
}
