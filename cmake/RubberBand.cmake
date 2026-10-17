# Rubber Band Library (GPLv2+, AGPL-compatible) built from its single-file compilation unit.
include(FetchContent)

FetchContent_Declare(
    rubberband
    URL https://github.com/breakfastquay/rubberband/archive/refs/tags/v4.0.0.tar.gz
    URL_HASH SHA256=24300f48a8014b7c863b573a9647e61b1b19b37875e2cdd92005e64c6424d266
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE
    SOURCE_SUBDIR no-cmake   # upstream builds with meson; we only need the sources
)
FetchContent_MakeAvailable(rubberband)

add_library(openmixer_rubberband STATIC ${rubberband_SOURCE_DIR}/single/RubberBandSingle.cpp)
target_include_directories(openmixer_rubberband SYSTEM PUBLIC ${rubberband_SOURCE_DIR})
set_target_properties(openmixer_rubberband PROPERTIES POSITION_INDEPENDENT_CODE ON)
if(MSVC)
    target_compile_definitions(openmixer_rubberband PRIVATE _USE_MATH_DEFINES NOMINMAX)
endif()
if(APPLE)
    target_link_libraries(openmixer_rubberband PUBLIC "-framework Accelerate")
endif()
# DSP-heavy third-party code: keep it optimised in Debug builds so tests and soak runs stay realtime.
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(openmixer_rubberband PRIVATE $<$<CONFIG:Debug>:-O2> -w)
endif()
