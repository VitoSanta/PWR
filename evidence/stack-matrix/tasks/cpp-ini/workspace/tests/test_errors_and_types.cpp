#include <stdexcept>

#include "check.hpp"
#include "ini/ini.hpp"

int main() {
  CHECK_THROWS_LINE(ini::Document::parse("a = 1\njust words\n"), 2);
  CHECK_THROWS_LINE(ini::Document::parse("\n\n = 1\n"), 3);
  CHECK_THROWS_LINE(ini::Document::parse("[open\n"), 1);
  CHECK_THROWS_LINE(ini::Document::parse("[   ]\n"), 1);
  CHECK_THROWS_LINE(ini::Document::parse("a = 1\nb = \"never closed\n"), 2);
  CHECK_THROWS_LINE(ini::Document::parse("a = \"x\" trailing\n"), 1);

  auto doc = ini::Document::parse(
      "[t]\nn = -42\nbig = +17\nbad = 4x\nyes = YES\noff = Off\none = 1\nmaybe = perhaps\n");
  CHECK_EQ(doc.get_int("t", "n").value_or(0), -42LL);
  CHECK_EQ(doc.get_int("t", "big").value_or(0), 17LL);
  CHECK(!doc.get_int("t", "absent").has_value());
  bool threw = false;
  try { doc.get_int("t", "bad"); } catch (const std::invalid_argument&) { threw = true; }
  CHECK(threw);
  CHECK(doc.get_bool("t", "yes").value_or(false));
  CHECK(!doc.get_bool("t", "off").value_or(true));
  CHECK(doc.get_bool("t", "one").value_or(false));
  threw = false;
  try { doc.get_bool("t", "maybe"); } catch (const std::invalid_argument&) { threw = true; }
  CHECK(threw);
  return REPORT();
}
