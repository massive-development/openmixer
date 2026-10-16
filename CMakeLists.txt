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

# Strict warnings for first-party sources only (JUCE module sources compiled into our
# targets are left alone). Usage: openmixer_enable_warnings(<sources...>)
# -Wold-style-cast is opt-in because JUCE's own headers are not clean under it:
#   cmake -B build -DOPENMIXER_STRICT_CASTS=ON 2>&1 and filter the build log for "/src/".
option(OPENMIXER_STRICT_CASTS "Warn on C-style casts (noisy: JUCE headers trigger it too)" OFF)
option(OPENMIXER_WERROR "Treat first-party warnings as errors" OFF)
function(openmixer_enable_warnings)
    if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
        set(flags -Wall -Wextra -Wpedantic -Wshadow -Wnon-virtual-dtor -Woverloaded-virtual)
        if(OPENMIXER_STRICT_CASTS)
            list(APPEND flags -Wold-style-cast)
        endif()
        if(OPENMIXER_WERROR)
            list(APPEND flags -Werror)
        endif()
        set_source_files_properties(${ARGN} PROPERTIES COMPILE_OPTIONS "${flags}")
    endif()
endfunction()

# Sanitizer builds for concurrent/real-time code:
#   cmake -B build-asan -DOPENMIXER_SANITIZE=address,undefined
#   cmake -B build-tsan -DOPENMIXER_SANITIZE=thread
set(OPENMIXER_SANITIZE "" CACHE STRING "Comma separated sanitizers (address,undefined | thread)")
if(OPENMIXER_SANITIZE AND CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    add_compile_options(-fsanitize=${OPENMIXER_SANITIZE} -fno-omit-frame-pointer)
    add_link_options(-fsanitize=${OPENMIXER_SANITIZE})
endif()

# =============================================================================
# Core (shared interfaces) and DSP libraries
# =============================================================================

add_library(openmixer_core INTERFACE)
target_include_directories(openmixer_core INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/src)
target_link_libraries(openmixer_core INTERFACE juce::juce_core)

add_library(openmixer_dsp STATIC src/dsp/Biquad.cpp src/dsp/Biquad.h)
target_include_directories(openmixer_dsp PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(src/dsp/Biquad.cpp)

# =============================================================================
# Engine library (audio device layer, deck, ring buffer)
# =============================================================================

add_library(openmixer_engine STATIC
    src/engine/ChannelStrip.cpp
    src/engine/ChannelStrip.h
    src/engine/Crossfader.cpp
    src/engine/Crossfader.h
    src/engine/Deck.cpp
    src/engine/Deck.h
    src/engine/DeckDecoder.cpp
    src/engine/DeckDecoder.h
    src/engine/AudioDevice.h
    src/engine/JuceAudioDevice.cpp
    src/engine/JuceAudioDevice.h
    src/engine/LatencyMonitor.cpp
    src/engine/LatencyMonitor.h
    src/engine/MockAudioDevice.cpp
    src/engine/MockAudioDevice.h
    src/engine/Mixer.cpp
    src/engine/Mixer.h
    src/engine/MixerParameters.cpp
    src/engine/PeakMeter.h
    src/engine/ProcessStage.h
    src/engine/RingBuffer.h
    src/engine/StageChain.cpp
    src/engine/StageChain.h
    src/engine/SweepFilter.cpp
    src/engine/SweepFilter.h
    src/engine/ThreeBandEq.cpp
    src/engine/ThreeBandEq.h
)

target_include_directories(openmixer_engine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(
    src/engine/ChannelStrip.cpp src/engine/Crossfader.cpp src/engine/Deck.cpp
    src/engine/DeckDecoder.cpp src/engine/JuceAudioDevice.cpp src/engine/LatencyMonitor.cpp
    src/engine/MockAudioDevice.cpp src/engine/Mixer.cpp src/engine/MixerParameters.cpp
    src/engine/StageChain.cpp src/engine/SweepFilter.cpp src/engine/ThreeBandEq.cpp)

target_link_libraries(openmixer_engine PUBLIC
    openmixer_core
    openmixer_dsp
    juce::juce_audio_basics
    juce::juce_audio_devices
    juce::juce_audio_formats
    juce::juce_core
    juce::juce_events
    juce::juce_recommended_config_flags
)

# =============================================================================
# Controllers library (MIDI mapping, trace harness)
# =============================================================================

add_library(openmixer_controllers STATIC
    src/controllers/MidiEvent.cpp
    src/controllers/MidiEvent.h
    src/controllers/MidiInputRouter.cpp
    src/controllers/MidiInputRouter.h
    src/controllers/MidiMap.cpp
    src/controllers/MidiMap.h
    src/controllers/MidiMapper.cpp
    src/controllers/MidiMapper.h
    src/controllers/MidiTrace.cpp
    src/controllers/MidiTrace.h
    src/controllers/ParameterRouter.cpp
    src/controllers/ParameterRouter.h
    src/controllers/RecordingTarget.h
)

target_include_directories(openmixer_controllers PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(
    src/controllers/MidiEvent.cpp src/controllers/MidiInputRouter.cpp src/controllers/MidiMap.cpp
    src/controllers/MidiMapper.cpp src/controllers/MidiTrace.cpp src/controllers/ParameterRouter.cpp)

target_link_libraries(openmixer_controllers PUBLIC
    openmixer_core
    juce::juce_audio_devices
    juce::juce_audio_basics
    juce::juce_core
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
    tests/unit/MidiMapperTest.cpp
)

juce_generate_juce_header(openmixer_tests)

target_include_directories(openmixer_tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)

target_compile_definitions(openmixer_tests PRIVATE
    OPENMIXER_TEST_DATA_DIR="${CMAKE_CURRENT_SOURCE_DIR}/tests/data")

target_link_libraries(openmixer_tests PRIVATE
    openmixer_controllers
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
