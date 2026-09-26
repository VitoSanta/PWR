#pragma once

#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace ini {

struct ParseError : std::runtime_error {
  int line;
  ParseError(int line, const std::string& message)
      : std::runtime_error("line " + std::to_string(line) + ": " + message), line(line) {}
};

class Document {
 public:
  static Document parse(std::string_view text);

  std::optional<std::string> get(std::string_view section, std::string_view key) const;
  std::string get_or(std::string_view section, std::string_view key, std::string_view fallback) const;
  std::optional<long long> get_int(std::string_view section, std::string_view key) const;
  std::optional<bool> get_bool(std::string_view section, std::string_view key) const;

  std::vector<std::string> sections() const;
  std::vector<std::string> keys(std::string_view section) const;
  std::string dump() const;

 private:
  struct Section {
    std::string name;
    std::vector<std::pair<std::string, std::string>> entries;
  };
  Section& section(const std::string& name);
  const Section* find(std::string_view name) const;
  std::vector<Section> sections_{Section{"", {}}};
};

}  // namespace ini
