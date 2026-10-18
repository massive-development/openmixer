cmake_minimum_required(VERSION 3.22)
project(openmixer VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Include our custom modules
list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")

# Platform-specific settings (before any dependency target: targets copy these when created)
if(APPLE)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "10.15" CACHE STRING "Minimum macOS deployment target")
    set(CMAKE_OSX_ARCHITECTURES "x86_64;arm64")
elseif(WIN32)
    set(CMAKE_WINDOWS_EXPORT_ALL_SYMBOLS ON)
endif()

# FetchContent for JUCE and dependencies
include(FetchContent)
include(Dependencies)
include(RubberBand)

# Configure build type
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
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

# Sanitizer builds (TSan cannot combine with ASan):
#   cmake -B build-asan -DENABLE_ASAN=ON -DENABLE_UBSAN=ON
#   cmake -B build-tsan -DENABLE_TSAN=ON   (run tests under `setarch $(uname -m) -R` on WSL2)
option(ENABLE_ASAN "Build with AddressSanitizer" OFF)
option(ENABLE_UBSAN "Build with UndefinedBehaviorSanitizer" OFF)
option(ENABLE_TSAN "Build with ThreadSanitizer" OFF)
set(OPENMIXER_SANITIZE "" CACHE STRING "Comma separated sanitizers (legacy; prefer ENABLE_*)")
set(_om_sanitizers ${OPENMIXER_SANITIZE})
if(ENABLE_ASAN)
    list(APPEND _om_sanitizers address)
endif()
if(ENABLE_UBSAN)
    list(APPEND _om_sanitizers undefined)
endif()
if(ENABLE_TSAN)
    list(APPEND _om_sanitizers thread)
endif()
if("thread" IN_LIST _om_sanitizers AND "address" IN_LIST _om_sanitizers)
    message(FATAL_ERROR "ENABLE_TSAN cannot be combined with ENABLE_ASAN")
endif()
if(_om_sanitizers AND CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    list(JOIN _om_sanitizers "," _om_sanitizer_list)
    add_compile_options(-fsanitize=${_om_sanitizer_list} -fno-omit-frame-pointer)
    add_link_options(-fsanitize=${_om_sanitizer_list})
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
    src/engine/DeckControl.cpp
    src/engine/DeckControl.h
    src/engine/FxRouting.cpp
    src/engine/FxRouting.h
    src/engine/DeckDecoder.cpp
    src/engine/DeckPlayback.cpp
    src/engine/DeckKeylock.cpp
    src/engine/TimeStretch.cpp
    src/engine/TimeStretch.h
    src/engine/FrameRing.h
    src/engine/HotcueBank.h
    src/engine/LoopPrefetch.cpp
    src/engine/LoopPrefetch.h
    src/engine/LoopState.h
    src/engine/DeckDecoder.h
    src/engine/DjEngine.cpp
    src/engine/DjEngine.h
    src/engine/AudioDevice.h
    src/engine/BeatGrid.h
    src/engine/Quantizer.cpp
    src/engine/Quantizer.h
    src/engine/SlipTracker.h
    src/engine/Sync.cpp
    src/engine/Sync.h
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
    src/engine/ChannelStrip.cpp src/engine/Crossfader.cpp src/engine/Deck.cpp src/engine/DeckControl.cpp src/engine/FxRouting.cpp
    src/engine/DeckDecoder.cpp src/engine/DeckPlayback.cpp src/engine/LoopPrefetch.cpp src/engine/DeckKeylock.cpp src/engine/TimeStretch.cpp src/engine/DjEngine.cpp src/engine/JuceAudioDevice.cpp src/engine/LatencyMonitor.cpp
    src/engine/MockAudioDevice.cpp src/engine/Mixer.cpp src/engine/MixerParameters.cpp
    src/engine/Quantizer.cpp src/engine/StageChain.cpp src/engine/SweepFilter.cpp src/engine/Sync.cpp src/engine/ThreeBandEq.cpp)

target_link_libraries(openmixer_engine PUBLIC
    openmixer_core
    openmixer_fx
    openmixer_dsp
    openmixer_rubberband
    juce::juce_audio_basics
    juce::juce_audio_devices
    juce::juce_audio_formats
    juce::juce_core
    juce::juce_events
    juce::juce_recommended_config_flags
)

# =============================================================================
# FX library (effects rack; ProcessStage inserts, independent of the mixer core)
# =============================================================================

add_library(openmixer_fx STATIC
    src/engine/fx/CrushFx.cpp
    src/engine/fx/CrushFx.h
    src/engine/fx/DelayLine.cpp
    src/engine/fx/DelayLine.h
    src/engine/fx/EchoFx.cpp
    src/engine/fx/EchoFx.h
    src/engine/fx/Effect.cpp
    src/engine/fx/Effect.h
    src/engine/fx/FlangerFx.cpp
    src/engine/fx/FlangerFx.h
    src/engine/fx/FxFactory.cpp
    src/engine/fx/FxFactory.h
    src/engine/fx/FxPresets.cpp
    src/engine/fx/FxPresets.h
    src/engine/fx/FxRack.cpp
    src/engine/fx/FxRack.h
    src/engine/fx/FxUnit.cpp
    src/engine/fx/FxUnit.h
    src/engine/fx/GateFx.cpp
    src/engine/fx/GateFx.h
    src/engine/fx/PhaserFx.cpp
    src/engine/fx/PhaserFx.h
    src/engine/fx/ReverbFx.cpp
    src/engine/fx/ReverbFx.h
    src/engine/fx/RollFx.cpp
    src/engine/fx/RollFx.h
    src/engine/fx/TapeStopFx.cpp
    src/engine/fx/TapeStopFx.h
    src/engine/fx/FxMath.h
    src/engine/fx/ReverbParts.h
)

target_include_directories(openmixer_fx PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(src/engine/fx/CrushFx.cpp src/engine/fx/DelayLine.cpp src/engine/fx/EchoFx.cpp src/engine/fx/Effect.cpp src/engine/fx/FlangerFx.cpp src/engine/fx/FxFactory.cpp src/engine/fx/FxPresets.cpp src/engine/fx/FxRack.cpp src/engine/fx/FxUnit.cpp src/engine/fx/GateFx.cpp src/engine/fx/PhaserFx.cpp src/engine/fx/ReverbFx.cpp src/engine/fx/RollFx.cpp src/engine/fx/TapeStopFx.cpp)

# Only header-only engine interfaces (ProcessStage, BeatGrid) are used, so no engine link.
target_link_libraries(openmixer_fx PUBLIC
    openmixer_core
    openmixer_dsp
    juce::juce_audio_basics
    juce::juce_core
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
# Analysis library (background jobs, result cache, analysers). Depends on core + dsp;
# engine adapters include engine headers that are header-only interfaces.
# =============================================================================

set(OPENMIXER_ANALYSIS_SOURCES
    src/analysis/AnalysisJob.cpp
    src/analysis/AnalysisService.cpp
    src/analysis/AnalyzerRegistry.cpp
    src/analysis/BeatGridModel.cpp
    src/analysis/BpmEstimator.cpp
    src/analysis/Chromagram.cpp
    src/analysis/ClipSource.cpp
    src/analysis/JobRunner.cpp
    src/analysis/KeyEstimator.cpp
    src/analysis/MusicalKey.cpp
    src/analysis/OnsetEnvelope.cpp
    src/analysis/OnsetPicker.cpp
    src/analysis/Segmenter.cpp
    src/analysis/SqliteResultCache.cpp
    src/analysis/Stft.cpp
    src/analysis/StructureAnalyzer.cpp
    src/analysis/TempoSearch.cpp
    src/analysis/WaveformBuilder.cpp
    src/analysis/WaveformData.cpp
)

add_library(openmixer_analysis STATIC
    ${OPENMIXER_ANALYSIS_SOURCES}
    src/analysis/AnalysedBeatGrid.h
    src/analysis/AnalysisJob.h
    src/analysis/AnalysisService.h
    src/analysis/Analyzer.h
    src/analysis/AnalyzerRegistry.h
    src/analysis/AudioClip.h
    src/analysis/BeatGridModel.h
    src/analysis/BpmEstimator.h
    src/analysis/Chromagram.h
    src/analysis/ClipSource.h
    src/analysis/JobRunner.h
    src/analysis/KeyEstimator.h
    src/analysis/MusicalKey.h
    src/analysis/OnsetEnvelope.h
    src/analysis/OnsetPicker.h
    src/analysis/ResultCache.h
    src/analysis/Segmenter.h
    src/analysis/SqliteResultCache.h
    src/analysis/Stft.h
    src/analysis/StructureAnalyzer.h
    src/analysis/TempoSearch.h
    src/analysis/WaveformBuilder.h
    src/analysis/WaveformData.h
)

target_include_directories(openmixer_analysis PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(${OPENMIXER_ANALYSIS_SOURCES})

target_link_libraries(openmixer_analysis
    PUBLIC
        openmixer_core
        openmixer_dsp
        juce::juce_audio_basics
        juce::juce_audio_formats
        juce::juce_core
        juce::juce_cryptography
        juce::juce_dsp
        juce::juce_recommended_config_flags
    PRIVATE
        openmixer_sqlite3
)

# =============================================================================
# Library (SQLite schema + DAOs, metadata ingest, watch folders). Depends on core; analysis (same tier) for the
# content-hash helper and the prep queue, which submits to AnalysisService.
# =============================================================================

set(OPENMIXER_LIBRARY_SOURCES
    src/library/CueDao.cpp
    src/library/FileUrl.cpp
    src/library/ImportCues.cpp
    src/library/ImportMerge.cpp
    src/library/ImportWriter.cpp
    src/library/ItunesImporter.cpp
    src/library/LibraryImport.cpp
    src/library/LibraryImporter.cpp
    src/library/PlistReader.cpp
    src/library/RekordboxImporter.cpp
    src/library/TrackKey.cpp
    src/library/Ingestor.cpp
    src/library/JuceMetadataReader.cpp
    src/library/Database.cpp
    src/library/Library.cpp
    src/library/MetadataReader.cpp
    src/library/Migrations.cpp
    src/library/OrderedList.cpp
    src/library/PlaylistDao.cpp
    src/library/PrepQueue.cpp
    src/library/PrepQueueDao.cpp
    src/library/PrepResults.cpp
    src/library/SearchCompiler.cpp
    src/library/SearchEngine.cpp
    src/library/SearchFields.cpp
    src/library/SearchKeys.cpp
    src/library/SearchParser.cpp
    src/library/SearchTerms.cpp
    src/library/Statement.cpp
    src/library/TrackDao.cpp
    src/library/TrackFileDao.cpp
    src/library/WatchFolderDao.cpp
    src/library/WatchFolders.cpp
)

add_library(openmixer_library STATIC
    ${OPENMIXER_LIBRARY_SOURCES}
    src/library/CueDao.h
    src/library/FileUrl.h
    src/library/ImportCues.h
    src/library/ImportMerge.h
    src/library/ImportWriter.h
    src/library/ItunesImporter.h
    src/library/LibraryImport.h
    src/library/LibraryImporter.h
    src/library/PlistReader.h
    src/library/RekordboxImporter.h
    src/library/ImportModel.h
    src/library/Database.h
    src/library/Ingestor.h
    src/library/JuceMetadataReader.h
    src/library/Library.h
    src/library/MetadataReader.h
    src/library/Migrations.h
    src/library/OrderedList.h
    src/library/PlaylistDao.h
    src/library/PrepQueue.h
    src/library/PrepQueueDao.h
    src/library/PrepResults.h
    src/library/SearchCompiler.h
    src/library/SearchEngine.h
    src/library/SearchFields.h
    src/library/SearchKeys.h
    src/library/SearchParser.h
    src/library/SearchQuery.h
    src/library/Statement.h
    src/library/TrackDao.h
    src/library/TrackFileDao.h
    src/library/TrackRecord.h
    src/library/WatchFolderDao.h
    src/library/WatchFolders.h
)

target_include_directories(openmixer_library PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(${OPENMIXER_LIBRARY_SOURCES})
target_link_libraries(openmixer_library
    PUBLIC
        openmixer_core
        openmixer_analysis
        juce::juce_audio_formats
        juce::juce_core
        juce::juce_recommended_config_flags
    PRIVATE
        openmixer_sqlite3
)

# =============================================================================
# Stems library (separator interface, licence-gated model download). Depends on analysis.
# =============================================================================

set(OPENMIXER_STEMS_SOURCES
    src/stems/ModelManifest.cpp
    src/stems/StemSeparator.cpp
    src/stems/WeightManager.cpp
)

add_library(openmixer_stems STATIC
    ${OPENMIXER_STEMS_SOURCES}
    src/stems/ModelManifest.h
    src/stems/StemSeparator.h
    src/stems/WeightManager.h
)

target_include_directories(openmixer_stems PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(${OPENMIXER_STEMS_SOURCES})
target_link_libraries(openmixer_stems PUBLIC openmixer_analysis juce::juce_core juce::juce_recommended_config_flags)

# =============================================================================
# Plugin API + registry (extension point; links engine and controllers one-way)
# =============================================================================

add_library(openmixer_plugins STATIC
    src/plugins/PluginApi.h
    src/plugins/PluginLoader.cpp
    src/plugins/PluginLoader.h
    src/plugins/PluginRegistry.cpp
    src/plugins/PluginRegistry.h
    src/plugins/StageCatalog.cpp
    src/plugins/StageCatalog.h
)

target_include_directories(openmixer_plugins PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(src/plugins/PluginLoader.cpp src/plugins/PluginRegistry.cpp src/plugins/StageCatalog.cpp)

target_link_libraries(openmixer_plugins PUBLIC
    openmixer_engine
    openmixer_controllers
    juce::juce_core
    juce::juce_recommended_config_flags
)

# Example plugin: a shared library loaded at runtime (also used by the tests)
add_library(openmixer_example_gain MODULE examples/plugins/GainPlugin.cpp)
target_include_directories(openmixer_example_gain PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)
set_target_properties(openmixer_example_gain PROPERTIES PREFIX "" CXX_VISIBILITY_PRESET hidden)
openmixer_enable_warnings(examples/plugins/GainPlugin.cpp)
target_link_libraries(openmixer_example_gain PRIVATE juce::juce_core juce::juce_recommended_config_flags)

# =============================================================================
# UI library (debug panel; real UI arrives in Wave 5)
# =============================================================================

add_library(openmixer_ui STATIC
    src/ui/ChannelStripPanel.cpp
    src/ui/ChannelStripPanel.h
    src/ui/ControlCaption.h
    src/ui/DebugPanel.cpp
    src/ui/DebugPanel.h
    src/ui/DeckStrip.cpp
    src/ui/DeckStrip.h
    src/ui/GlassLookAndFeel.cpp
    src/ui/GlassLookAndFeel.h
    src/ui/MeterBar.cpp
    src/ui/MeterBar.h
    src/ui/Theme.h
    src/ui/ValueFormat.h
)

target_include_directories(openmixer_ui PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
openmixer_enable_warnings(
    src/ui/ChannelStripPanel.cpp src/ui/DebugPanel.cpp src/ui/DeckStrip.cpp src/ui/GlassLookAndFeel.cpp
    src/ui/MeterBar.cpp)

target_link_libraries(openmixer_ui PUBLIC
    openmixer_engine
    juce::juce_gui_basics
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
    openmixer_ui
    openmixer_controllers
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
set(OPENMIXER_CURL_LIBS "")
if(NOT WIN32)
    find_package(CURL REQUIRED)
    set(OPENMIXER_CURL_LIBS CURL::libcurl)
endif()
target_link_libraries(openmixer PRIVATE ${OPENMIXER_CURL_LIBS})

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

FetchContent_Declare(
    googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.17.0.zip
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE
)
set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(googletest)
include(GoogleTest)

juce_add_console_app(openmixer_tests
    PRODUCT_NAME "OpenMixer Tests"
)

target_sources(openmixer_tests PRIVATE
    tests/unit/TestMain.cpp
    tests/unit/SmokeTest.cpp
    tests/unit/RingBufferTest.cpp
    tests/unit/AudioDeviceTest.cpp
    tests/unit/DeckTest.cpp
    tests/unit/DeckLoopTest.cpp
    tests/unit/DeckKeylockTest.cpp
    tests/unit/DeckTestUtil.h
    tests/unit/MixerTest.cpp
    tests/unit/MidiMapperTest.cpp
    tests/unit/DjEngineTest.cpp
    tests/unit/DebugPanelTest.cpp
    tests/unit/ValueFormatTest.cpp
    tests/unit/SyncTest.cpp
    tests/unit/PluginRegistryTest.cpp
    tests/unit/FxTest.cpp
    tests/unit/EngineWiringTest.cpp
    tests/unit/AnalysisCacheTest.cpp
    tests/unit/AnalysisServiceTest.cpp
    tests/unit/BpmEstimatorTest.cpp
    tests/unit/KeyEstimatorTest.cpp
    tests/unit/WaveformBuilderTest.cpp
    tests/unit/StructureAnalyzerTest.cpp
    tests/unit/StemsTest.cpp
    tests/unit/LibraryDatabaseTest.cpp
    tests/unit/LibraryDaoTest.cpp
    tests/unit/LibraryIngestTest.cpp
    tests/unit/LibraryWatchFoldersTest.cpp
    tests/unit/LibrarySearchTest.cpp
    tests/unit/LibraryImportTest.cpp
    tests/unit/LibraryPrepQueueTest.cpp
)

juce_generate_juce_header(openmixer_tests)

target_include_directories(openmixer_tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)

target_compile_definitions(openmixer_tests PRIVATE
    OPENMIXER_TEST_DATA_DIR="${CMAKE_CURRENT_SOURCE_DIR}/tests/data"
    OPENMIXER_EXAMPLE_PLUGIN_PATH="$<TARGET_FILE:openmixer_example_gain>")
add_dependencies(openmixer_tests openmixer_example_gain)

target_link_libraries(openmixer_tests PRIVATE
    openmixer_library
    openmixer_stems
    openmixer_analysis
    openmixer_plugins
    openmixer_fx
    openmixer_ui
    openmixer_controllers
    openmixer_engine
    ${OPENMIXER_CURL_LIBS}
    juce::juce_audio_basics
    juce::juce_audio_devices
    juce::juce_audio_formats
    juce::juce_core
    juce::juce_gui_basics
    GTest::gtest
    GTest::gmock
)

# Suites named *Slow use real threads/timing -> label "slow"; the rest -> "unit".
gtest_discover_tests(openmixer_tests
    TEST_FILTER "-*Slow.*"
    PROPERTIES LABELS unit TIMEOUT 60
    DISCOVERY_MODE PRE_TEST
    DISCOVERY_TIMEOUT 60
)
gtest_discover_tests(openmixer_tests
    TEST_FILTER "*Slow.*"
    PROPERTIES LABELS slow TIMEOUT 120
    DISCOVERY_MODE PRE_TEST
    DISCOVERY_TIMEOUT 60
)

# Short headless soak (label "soak"); the 10-minute Wave 2 gate (Todo 13) is soak_10min below
add_executable(openmixer_soak tools/soak_mix.cpp tools/soak/OutputChecker.h tools/soak/Scenario.h)
target_include_directories(openmixer_soak PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src ${CMAKE_CURRENT_SOURCE_DIR}/tools)
openmixer_enable_warnings(tools/soak_mix.cpp)
target_link_libraries(openmixer_soak PRIVATE
    openmixer_fx
    openmixer_engine
    ${OPENMIXER_CURL_LIBS}
    juce::juce_audio_formats
    juce::juce_audio_devices
    juce::juce_audio_basics
    juce::juce_events
    juce::juce_core
    juce::juce_recommended_config_flags
)
# Shared CI runners hiccup (one 15 ms callback seen on Windows); allow a few xruns, systemic ones still fail
add_test(NAME soak_short COMMAND openmixer_soak --seconds 30 --max-xruns 5)
set_tests_properties(soak_short PROPERTIES LABELS soak TIMEOUT 120)
# Wave 2 gate (Todo 13): 10 minutes, so opt-in: cmake -DOPENMIXER_LONG_SOAK=ON, then ctest -L soak_long
option(OPENMIXER_LONG_SOAK "Register the 10-minute soak test" OFF)
if(OPENMIXER_LONG_SOAK)
    add_test(NAME soak_10min COMMAND openmixer_soak --seconds 600 --max-xruns 5)
    set_tests_properties(soak_10min PROPERTIES LABELS soak_long TIMEOUT 720)
endif()

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
    BUNDLE DESTINATION .
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
)

install(TARGETS openmixer_tests
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)
