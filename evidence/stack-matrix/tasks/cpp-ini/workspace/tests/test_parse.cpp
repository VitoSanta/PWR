#include "check.hpp"
#include "ini/ini.hpp"

int main() {
  auto doc = ini::Document::parse(
      "; the service\n"
      "name = api\n"
      "\n"
      "[server]\n"
      "  host =  0.0.0.0  \n"
      "port=8080 ; the default\n"
      "# a comment\n"
      "[ database ]\n"
      "url = \"postgres://db;x\" # quoted keeps ;\n"
      "motd = \"  hello\\n \\\"world\\\" \"\n");
  CHECK_EQ(doc.get("", "name").value_or("?"), std::string("api"));
  CHECK_EQ(doc.get("server", "host").value_or("?"), std::string("0.0.0.0"));
  CHECK_EQ(doc.get("server", "port").value_or("?"), std::string("8080"));
  CHECK_EQ(doc.get("database", "url").value_or("?"), std::string("postgres://db;x"));
  CHECK_EQ(doc.get("database", "motd").value_or("?"), std::string("  hello\n \"world\" "));
  CHECK(!doc.get("server", "missing").has_value());
  CHECK(!doc.get("nowhere", "name").has_value());
  CHECK(!doc.get("Server", "host").has_value());
  CHECK_EQ(doc.get_or("server", "timeout", "30"), std::string("30"));

  auto merged = ini::Document::parse("[a]\nx = 1\ny = 2\n[b]\nz = 3\n[a]\nx = 9\nw = 4\n");
  CHECK_EQ(merged.get("a", "x").value_or("?"), std::string("9"));
  auto keys = merged.keys("a");
  CHECK_EQ(keys.size(), std::size_t{3});
  if (keys.size() == 3) {
    CHECK_EQ(keys[0], std::string("x"));
    CHECK_EQ(keys[1], std::string("y"));
    CHECK_EQ(keys[2], std::string("w"));
  }
  auto sections = merged.sections();
  CHECK_EQ(sections.size(), std::size_t{2});
  CHECK(merged.keys("zzz").empty());

  auto with_global = ini::Document::parse("k = v\n[s]\na = b\n");
  CHECK_EQ(with_global.sections().size(), std::size_t{2});
  CHECK_EQ(with_global.sections()[0], std::string(""));
  CHECK(ini::Document::parse("[s]\na = b\n").sections() == std::vector<std::string>{"s"});
  return REPORT();
}
