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
