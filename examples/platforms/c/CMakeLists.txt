cmake_minimum_required(VERSION 3.20)
project(boltffi_c_demo LANGUAGES C)

find_package(demo CONFIG REQUIRED)

set(demo_sources
    tests/demo.c
    tests/scalars.c
    tests/results.c
    tests/classes.c
    tests/options.c
    tests/strings.c
    tests/records.c
    tests/vectors.c
    tests/builtins.c
    tests/enums.c
    tests/owned_records.c
    tests/constants.c
    tests/callbacks.c
    tests/scalar_enums.c
)
add_executable(c_demo ${demo_sources})
set_target_properties(c_demo PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED YES C_EXTENSIONS NO)
target_link_libraries(c_demo PRIVATE demo::demo)

add_executable(c_example example.c)
set_target_properties(c_example PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED YES C_EXTENSIONS NO)
target_link_libraries(c_example PRIVATE demo::demo)

if(WIN32)
    add_custom_command(TARGET c_demo POST_BUILD
        COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:demo::demo>" "$<TARGET_FILE_DIR:c_demo>")
    add_custom_command(TARGET c_example POST_BUILD
        COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:demo::demo>" "$<TARGET_FILE_DIR:c_example>")
else()
    target_link_libraries(c_demo PRIVATE m)
    target_link_libraries(c_example PRIVATE m)
endif()

add_executable(c_demo_static ${demo_sources})
set_target_properties(c_demo_static PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED YES C_EXTENSIONS NO)
target_link_libraries(c_demo_static PRIVATE demo::demo_static)

add_executable(c_example_static example.c)
set_target_properties(c_example_static PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED YES C_EXTENSIONS NO)
target_link_libraries(c_example_static PRIVATE demo::demo_static)

if(NOT WIN32)
    target_link_libraries(c_demo_static PRIVATE m)
    target_link_libraries(c_example_static PRIVATE m)
endif()

enable_testing()
add_test(NAME c_demo COMMAND c_demo)
add_test(NAME c_example COMMAND c_example)
add_test(NAME c_demo_static COMMAND c_demo_static)
add_test(NAME c_example_static COMMAND c_example_static)

find_package(PkgConfig QUIET)
if(PkgConfig_FOUND AND NOT MSVC)
    pkg_check_modules(demo_pkg REQUIRED IMPORTED_TARGET demo)
    add_executable(c_example_pkgconfig example.c)
    set_target_properties(c_example_pkgconfig PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED YES C_EXTENSIONS NO)
    target_link_libraries(c_example_pkgconfig PRIVATE PkgConfig::demo_pkg)
    add_test(NAME c_example_pkgconfig COMMAND c_example_pkgconfig)

    pkg_check_modules(demo_static_pkg REQUIRED IMPORTED_TARGET demo-static)
    add_executable(c_example_pkgconfig_static example.c)
    set_target_properties(c_example_pkgconfig_static PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED YES C_EXTENSIONS NO)
    target_link_libraries(c_example_pkgconfig_static PRIVATE PkgConfig::demo_static_pkg)
    add_test(NAME c_example_pkgconfig_static COMMAND c_example_pkgconfig_static)
endif()
