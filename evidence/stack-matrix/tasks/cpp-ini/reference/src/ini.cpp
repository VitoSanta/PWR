#include "ini/ini.hpp"

#include <cctype>

namespace ini {
namespace {

bool space(char c) { return std::isspace(static_cast<unsigned char>(c)) != 0; }

std::string_view trim(std::string_view s) {
  while (!s.empty() && space(s.front())) s.remove_prefix(1);
  while (!s.empty() && space(s.back())) s.remove_suffix(1);
  return s;
}

bool comment_or_empty(std::string_view rest) {
  rest = trim(rest);
  return rest.empty() || rest.front() == ';' || rest.front() == '#';
}

std::string lower(std::string_view s) {
  std::string out(s);
  for (char& c : out) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  return out;
}

bool needs_quotes(const std::string& value) {
  if (value.empty() || space(value.front()) || space(value.back())) return true;
  for (char c : value) {
    if (c == ';' || c == '#' || c == '"' || c == '\\' || c == '\n') return true;
  }
  return false;
}

std::string quoted(const std::string& value) {
  std::string out = "\"";
  for (char c : value) {
    if (c == '"') out += "\\\"";
    else if (c == '\\') out += "\\\\";
    else if (c == '\n') out += "\\n";
    else out += c;
  }
  return out + "\"";
}

}  // namespace

Document::Section& Document::section(const std::string& name) {
  for (auto& s : sections_) {
    if (s.name == name) return s;
  }
  sections_.push_back(Section{name, {}});
  return sections_.back();
}

const Document::Section* Document::find(std::string_view name) const {
  for (const auto& s : sections_) {
    if (s.name == name) return &s;
  }
  return nullptr;
}

Document Document::parse(std::string_view text) {
  Document doc;
  std::string current;
  int number = 0;
  while (!text.empty() || number == 0) {
    ++number;
    auto end = text.find('\n');
    std::string_view raw = text.substr(0, end);
    text = end == std::string_view::npos ? std::string_view{} : text.substr(end + 1);
    std::string_view line = trim(raw);
    if (line.empty() || line.front() == ';' || line.front() == '#') {
      if (text.empty()) break;
      continue;
    }
    if (line.front() == '[') {
      auto close = line.find(']');
      if (close == std::string_view::npos) throw ParseError(number, "header without ]");
      std::string name(trim(line.substr(1, close - 1)));
      if (name.empty()) throw ParseError(number, "header without a name");
      if (!comment_or_empty(line.substr(close + 1))) throw ParseError(number, "text after a header");
      current = name;
      doc.section(current);
      if (text.empty()) break;
      continue;
    }
    auto eq = line.find('=');
    if (eq == std::string_view::npos) throw ParseError(number, "expected key = value");
    std::string key(trim(line.substr(0, eq)));
    if (key.empty()) throw ParseError(number, "empty key");
    std::string_view after = line.substr(eq + 1);
    std::string value;
    std::string_view start = trim(after);
    if (!start.empty() && start.front() == '"') {
      std::size_t i = 1;
      bool closed = false;
      for (; i < start.size(); ++i) {
        char c = start[i];
        if (c == '\\' && i + 1 < start.size()) {
          char next = start[++i];
          value += next == 'n' ? '\n' : next;
        } else if (c == '"') {
          closed = true;
          break;
        } else {
          value += c;
        }
      }
      if (!closed) throw ParseError(number, "unterminated quote");
      if (!comment_or_empty(start.substr(i + 1))) throw ParseError(number, "text after a quoted value");
    } else {
      std::size_t cut = after.size();
      for (std::size_t i = 1; i < after.size(); ++i) {
        if ((after[i] == ';' || after[i] == '#') && space(after[i - 1])) {
          cut = i;
          break;
        }
      }
      value = std::string(trim(after.substr(0, cut)));
    }
    auto& entries = doc.section(current).entries;
    bool replaced = false;
    for (auto& entry : entries) {
      if (entry.first == key) {
        entry.second = value;
        replaced = true;
      }
    }
    if (!replaced) entries.emplace_back(key, value);
    if (text.empty()) break;
  }
  return doc;
}

std::optional<std::string> Document::get(std::string_view section, std::string_view key) const {
  if (const auto* s = find(section)) {
    for (const auto& entry : s->entries) {
      if (entry.first == key) return entry.second;
    }
  }
  return std::nullopt;
}

std::string Document::get_or(std::string_view section, std::string_view key, std::string_view fallback) const {
  auto value = get(section, key);
  return value ? *value : std::string(fallback);
}

std::optional<long long> Document::get_int(std::string_view section, std::string_view key) const {
  auto value = get(section, key);
  if (!value) return std::nullopt;
  std::string_view digits = *value;
  if (!digits.empty() && (digits.front() == '+' || digits.front() == '-')) digits.remove_prefix(1);
  if (digits.empty()) throw std::invalid_argument("not an integer: " + *value);
  for (char c : digits) {
    if (!std::isdigit(static_cast<unsigned char>(c))) throw std::invalid_argument("not an integer: " + *value);
  }
  return std::stoll(*value);
}

std::optional<bool> Document::get_bool(std::string_view section, std::string_view key) const {
  auto value = get(section, key);
  if (!value) return std::nullopt;
  auto word = lower(*value);
  if (word == "true" || word == "yes" || word == "on" || word == "1") return true;
  if (word == "false" || word == "no" || word == "off" || word == "0") return false;
  throw std::invalid_argument("not a boolean: " + *value);
}

std::vector<std::string> Document::sections() const {
  std::vector<std::string> names;
  for (const auto& s : sections_) {
    if (s.name.empty() && s.entries.empty()) continue;
    names.push_back(s.name);
  }
  return names;
}

std::vector<std::string> Document::keys(std::string_view section) const {
  std::vector<std::string> names;
  if (const auto* s = find(section)) {
    for (const auto& entry : s->entries) names.push_back(entry.first);
  }
  return names;
}

std::string Document::dump() const {
  std::string out;
  bool first = true;
  for (const auto& s : sections_) {
    if (s.name.empty() && s.entries.empty()) continue;
    if (!first) out += "\n";
    first = false;
    if (!s.name.empty()) out += "[" + s.name + "]\n";
    for (const auto& [key, value] : s.entries) {
      out += key + " = " + (needs_quotes(value) ? quoted(value) : value) + "\n";
    }
  }
  return out;
}

}  // namespace ini
