#include "check.hpp"
#include "ini/ini.hpp"

int main() {
  auto doc = ini::Document::parse(
      "[b]\nkey=value\n[a]\nempty = \"\"\npadded = \"  x \"\nsemi = \"a;b\"\n"
      "top = 1\n");
  std::string expected =
      "[b]\nkey = value\n\n[a]\nempty = \"\"\npadded = \"  x \"\nsemi = \"a;b\"\ntop = 1\n";
  CHECK_EQ(doc.dump(), expected);

  auto global = ini::Document::parse("z = 1\n[s]\nq = \"line\\nbreak\"\n");
  CHECK_EQ(global.dump(), std::string("z = 1\n\n[s]\nq = \"line\\nbreak\"\n"));

  auto again = ini::Document::parse(global.dump());
  CHECK_EQ(again.get("s", "q").value_or("?"), std::string("line\nbreak"));
  CHECK_EQ(again.dump(), global.dump());
  return REPORT();
}
