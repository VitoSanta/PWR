cmake_minimum_required(VERSION 3.20)
project(ini LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_library(ini src/ini.cpp)
target_include_directories(ini PUBLIC include)

enable_testing()
file(GLOB TEST_SOURCES CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/tests/*.cpp)
foreach(source ${TEST_SOURCES})
  get_filename_component(name ${source} NAME_WE)
  add_executable(${name} ${source})
  target_link_libraries(${name} PRIVATE ini)
  add_test(NAME ${name} COMMAND ${name})
endforeach()
