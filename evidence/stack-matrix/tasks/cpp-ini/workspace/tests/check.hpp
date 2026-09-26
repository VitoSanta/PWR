// A few assertion macros, so the tests need nothing beyond the compiler.
#pragma once
#include <iostream>
#include <string>

inline int failures = 0;

#define CHECK(condition)                                                              \
  do {                                                                                \
    if (!(condition)) {                                                               \
      std::cerr << __FILE__ << ":" << __LINE__ << ": CHECK(" #condition ") failed\n"; \
      ++failures;                                                                     \
    }                                                                                 \
  } while (0)

#define CHECK_EQ(actual, expected)                                                   \
  do {                                                                               \
    auto actual_ = (actual);                                                         \
    auto expected_ = (expected);                                                     \
    if (!(actual_ == expected_)) {                                                   \
      std::cerr << __FILE__ << ":" << __LINE__ << ": " #actual " was " << actual_   \
                << ", expected " << expected_ << "\n";                               \
      ++failures;                                                                    \
    }                                                                                \
  } while (0)

#define CHECK_THROWS_LINE(statement, expected_line)                                   \
  do {                                                                                \
    try {                                                                             \
      statement;                                                                      \
      std::cerr << __FILE__ << ":" << __LINE__ << ": " #statement " did not throw\n"; \
      ++failures;                                                                     \
    } catch (const ini::ParseError& error) {                                          \
      if (error.line != (expected_line)) {                                            \
        std::cerr << __FILE__ << ":" << __LINE__ << ": error on line " << error.line \
                  << ", expected " << (expected_line) << "\n";                        \
        ++failures;                                                                   \
      }                                                                               \
    }                                                                                 \
  } while (0)

#define REPORT() (failures == 0 ? 0 : (std::cerr << failures << " failed\n", 1))
