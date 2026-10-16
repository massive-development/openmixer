# ADR 0002: Technology Stack - JUCE with AGPLv3

## Status

Accepted

## Context

We need a cross-platform audio application framework that supports:
- Windows, macOS, Linux
- Real-time audio processing
- MIDI/HID controller support
- GUI with low latency requirements

## Decision

We use **JUCE 8.x** as our application framework under its AGPLv3 license.

### Why JUCE

- Mature, widely-used audio framework
- Cross-platform native GUI
- Built-in audio device management
- MIDI support
- DSP module for effects
- AGPLv3 compatible with our license choice

### Dependencies

| Library | License | Purpose |
|---------|---------|---------|
| JUCE 8.x | AGPLv3 | Core framework |
| Rubber Band 4.x | GPLv2+ | Key-lock time-stretch |
| FFmpeg 7.x | LGPL (LGPL build) | Audio decoding |
| libsndfile 1.2.x | LGPL | Audio file I/O |
| TagLib 2.x | LGPL/MPL | Metadata reading |
| SQLite 3.x | Public Domain | Library database |
| ONNX Runtime 1.x | MIT | Stem separation inference |
| Essentia (if used) | AGPLv3 | Audio analysis |

## Consequences

- Our application must be released under AGPLv3 (or compatible)
- All GPL dependencies must be properly attributed
- Build system must not include proprietary SDKs (ASIO) by default

## References

- JUCE License: https://github.com/juce-framework/JUCE/blob/master/LICENSE.md
- Rubber Band: https://github.com/breakfastquay/rubberband
