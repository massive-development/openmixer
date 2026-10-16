cmake_minimum_required(VERSION 3.22)
project(openmixer VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Include our custom modules
list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")

# FetchContent for JUCE and dependencies
include(FetchContent)
include(Dependencies)

# Configure build type
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# Platform-specific settings
if(APPLE)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "10.15" CACHE STRING "Minimum macOS deployment target")
    set(CMAKE_OSX_ARCHITECTURES "x86_64;arm64")
elseif(WIN32)
    set(CMAKE_WINDOWS_EXPORT_ALL_SYMBOLS ON)
endif()

# Enable position independent code
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

# Add GTK and WebKitGTK include paths for Linux (required by JUCE GUI modules)
if(UNIX AND NOT APPLE)
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(GTK3 REQUIRED gtk+-3.0)
    pkg_check_modules(WEBKIT REQUIRED webkit2gtk-4.1)
    include_directories(${GTK3_INCLUDE_DIRS} ${WEBKIT_INCLUDE_DIRS})
endif()

# JUCE is already pulled in via FetchContent_MakeAvailable
# The JUCE CMake functions should now be available

# =============================================================================
# Engine library (audio device layer, deck, ring buffer)
# =============================================================================

add_library(openmixer_engine STATIC
    src/engine/AudioDevice.cpp
    src/engine/AudioDevice.h
    src/engine/Deck.cpp
    src/engine/Deck.h
    src/engine/Mixer.cpp
    src/engine/Mixer.h
    src/engine/RingBuffer.h
)

target_include_directories(openmixer_engine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)

target_link_libraries(openmixer_engine PUBLIC
    juce::juce_audio_basics
    juce::juce_audio_devices
    juce::juce_audio_formats
    juce::juce_core
    juce::juce_events
    juce::juce_recommended_config_flags
)

# =============================================================================
# Main Application
# =============================================================================

juce_add_gui_app(openmixer
    PRODUCT_NAME "OpenMixer"
    COMPANY_NAME "OpenMixer"
    BUNDLE_ID "org.openmixer.app"
    VERSION ${PROJECT_VERSION}
)

target_sources(openmixer PRIVATE
    src/app/Main.cpp
    src/app/MainComponent.cpp
    src/app/MainComponent.h
)

juce_generate_juce_header(openmixer)

target_link_libraries(openmixer PRIVATE
    openmixer_engine
    juce::juce_audio_basics
    juce::juce_audio_devices
    juce::juce_audio_formats
    juce::juce_audio_utils
    juce::juce_core
    juce::juce_data_structures
    juce::juce_events
    juce::juce_graphics
    juce::juce_gui_basics
)

# Link required system libraries
find_package(CURL REQUIRED)
target_link_libraries(openmixer PRIVATE CURL::libcurl)

# Platform-specific settings
if(APPLE)
    set_target_properties(openmixer PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_GUI_IDENTIFIER "org.openmixer.app"
        MACOSX_BUNDLE_BUNDLE_NAME "OpenMixer"
    )
elseif(WIN32)
    set_target_properties(openmixer PROPERTIES
        WIN32_EXECUTABLE TRUE
    )
endif()

# =============================================================================
# Test Executable
# =============================================================================

enable_testing()

juce_add_console_app(openmixer_tests
    PRODUCT_NAME "OpenMixer Tests"
)

target_sources(openmixer_tests PRIVATE
    tests/unit/SmokeTest.cpp
    tests/unit/RingBufferTest.cpp
    tests/unit/AudioDeviceTest.cpp
    tests/unit/DeckTest.cpp
    tests/unit/MixerTest.cpp
)

juce_generate_juce_header(openmixer_tests)

target_include_directories(openmixer_tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)

target_link_libraries(openmixer_tests PRIVATE
    openmixer_engine
    CURL::libcurl
    juce::juce_audio_basics
    juce::juce_audio_devices
    juce::juce_audio_formats
    juce::juce_core
    juce::juce_gui_basics
)

# Register tests with CTest
add_test(NAME smoke COMMAND openmixer_tests)

# =============================================================================
# Tools
# =============================================================================

# License checker
add_custom_target(license-check
    COMMAND python3 ${CMAKE_CURRENT_SOURCE_DIR}/tools/license_check.py
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMENT "Checking third-party licenses..."
)

# =============================================================================
# Installation
# =============================================================================

include(GNUInstallDirs)

install(TARGETS openmixer
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
)

install(TARGETS openmixer_tests
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)
