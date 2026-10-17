# Dependencies and FetchContent declarations

include(FetchContent)
include(ExternalProject)

# Include dependency pins
include(JUCEPin)

function(fetch_with_license dep_name repo_url commit_or_tag)
    FetchContent_Declare(
        ${dep_name}
        GIT_REPOSITORY ${repo_url}
        GIT_TAG ${commit_or_tag}
        GIT_SHALLOW TRUE
    )
    FetchContent_MakeAvailable(${dep_name})
endfunction()

# JUCE - Core framework
# Disable building JUCE extras to avoid conflicts
set(JUCE_BUILD_EXTRAS OFF CACHE BOOL "Build JUCE extras" FORCE)
set(JUCE_BUILD_JUCEIDE OFF CACHE BOOL "Build JUCE IDE" FORCE)

FetchContent_Declare(
    JUCE
    GIT_REPOSITORY ${JUCE_REPO}
    GIT_TAG ${JUCE_COMMIT}
    GIT_SHALLOW TRUE
)
# This will clone and make JUCE available as a subdirectory
FetchContent_MakeAvailable(JUCE)

# Optional: ASIO support (not enabled by default)
if(OPENMIXER_ENABLE_ASIO)
    if(NOT DEFINED ASIO_SDK_PATH)
        message(FATAL_ERROR "ASIO_SDK_PATH must be set to use ASIO support")
    endif()
    include_directories(${ASIO_SDK_PATH})
endif()

# SQLite (public domain) - official amalgamation, compiled as a static C library.
# Pin: SQLITE_VERSION in JUCEPin.cmake; update the URL and hash together.
FetchContent_Declare(
    sqlite3_amalgamation
    URL https://www.sqlite.org/2024/sqlite-amalgamation-3460000.zip
    URL_HASH SHA256=712a7d09d2a22652fb06a49af516e051979a3984adb067da86760e60ed51a7f5
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE
)
FetchContent_MakeAvailable(sqlite3_amalgamation)

find_package(Threads REQUIRED)
add_library(openmixer_sqlite3 STATIC ${sqlite3_amalgamation_SOURCE_DIR}/sqlite3.c)
target_include_directories(openmixer_sqlite3 SYSTEM PUBLIC ${sqlite3_amalgamation_SOURCE_DIR})
target_compile_definitions(openmixer_sqlite3 PRIVATE
    SQLITE_THREADSAFE=1 SQLITE_DQS=0 SQLITE_OMIT_LOAD_EXTENSION SQLITE_DEFAULT_FOREIGN_KEYS=1)
target_link_libraries(openmixer_sqlite3 PUBLIC Threads::Threads ${CMAKE_DL_LIBS})
