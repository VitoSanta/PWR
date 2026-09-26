#!/bin/sh
# Configure, build and run every test.
set -e
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug >/dev/null
cmake --build build
ctest --test-dir build --output-on-failure
