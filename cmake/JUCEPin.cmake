# Dependency pins for OpenMixer

# JUCE (pinned to specific commit)
set(JUCE_REPO "https://github.com/juce-framework/JUCE.git")
set(JUCE_COMMIT "8.0.4")  # Update to latest stable 8.x tag

# Rubber Band
set(RUBBER_BAND_VERSION "4.3.0")

# FFmpeg (LGPL build only)
set(FFMPEG_VERSION "7.0.1")

# libsndfile
set(LIBSNDFILE_VERSION "1.2.2")

# TagLib
set(TAGLIB_VERSION "2.0.1")

# SQLite (amalgamation)
set(SQLITE_VERSION "3.46.0")

# ONNX Runtime
set(ONNXRUNTIME_VERSION "1.18.0")

# Essentia (optional, for analysis)
set(ESSENTIA_REPO "https://github.com/MTG/essentia.git")
set(ESSENTIA_COMMIT "2.1beta5")

# qm-dsp (alternative BPM detection)
set(QMDSP_REPO "https://code.soundsoftware.ac.uk/git/qm-dsp.git")
set(QMDSP_COMMIT "master")
