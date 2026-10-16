# JUCE CMake API Example
# https://github.com/juce-framework/JUCE/blob/master/docs/CMake%20API.md

# Add a GUI application
juce_add_gui_app(openmixer
    PRODUCT_NAME "OpenMixer"
    COMPANY_NAME "OpenMixer"
    BUNDLE_ID "org.openmixer.app"
    VERSION ${PROJECT_VERSION}
    COPY_ICON_TO_BUNDLE FALSE
)

# Add source files
target_sources(openmixer PRIVATE
    src/app/Main.cpp
    src/app/MainComponent.cpp
)

# Add JUCE modules
juce_add_modules(openmixer
    juce_audio_basics
    juce_audio_devices
    juce_audio_formats
    juce_audio_processors
    juce_audio_utils
    juce_core
    juce_data_structures
    juce_events
    juce_graphics
    juce_gui_basics
    juce_gui_extra
    juce_dsp
)

# Link to external libraries
target_link_libraries(openmixer PRIVATE
    juce::juce_audio_basics
    juce::juce_audio_devices
    juce::juce_audio_formats
    juce::juce_audio_utils
    juce::juce_core
    juce::juce_gui_basics
    juce::juce_gui_extra
)

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
elseif(UNIX)
    # Linux
    find_package(X11 REQUIRED)
    target_link_libraries(openmixer PRIVATE ${X11_LIBRARIES})
endif()

# Add build options
option(OPENMIXER_SMOKETEST "Build with smoke test mode" OFF)
if(OPENMIXER_SMOKETEST)
    target_compile_definitions(openmixer PRIVATE
        OPENMIXER_SMOKE=1
    )
endif()
