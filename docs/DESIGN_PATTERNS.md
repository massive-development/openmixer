# Design Patterns in Use

Registry for agents and contributors. Check here before starting a large implementation; add an entry when you introduce or adopt a pattern.

## Entry template

- **Pattern:** name
- **Where:** files/targets
- **Why:** problem it solves (one line)
- **Reuse when:** situations that should use it
- **Notes:** constraints (threading, ownership)

## Planned patterns

### Plugin registry / extension points
- **Where:** not yet implemented (planned, see `CONTRIBUTING.md` "Extensibility & User Freedom")
- **Why:** let users add audio processors, FX, analysers, importers, controller maps and themes without changing core.
- **Reuse when:** adding any feature a user might reasonably want to replace or extend. Expose it via an interface + registry, not a hardcoded path.

## Current patterns

### Interface + mock (Strategy / Adapter)
- **Where:** `src/engine/AudioDevice.h`, `JuceAudioDevice.h`, `MockAudioDevice.h`
- **Why:** swap hardware backends and run headless in CI.
- **Reuse when:** wrapping any external I/O (MIDI input, file/network, HID) so tests need no hardware.
- **Notes:** callers depend on the abstract interface only.

### Lock-free SPSC ring buffer
- **Where:** `src/engine/RingBuffer.h`
- **Why:** pass audio between a worker thread and the audio callback without locks or allocation.
- **Reuse when:** any realtime thread needs data from a non-realtime producer (decode, analysis results, MIDI events).
- **Notes:** exactly one producer and one consumer thread.

### Epoch handshake (producer-owned reset)
- **Where:** `src/engine/Deck.h`, `DeckDecoder.cpp`
- **Why:** seek/load without the producer resetting a ring the consumer is reading.
- **Reuse when:** a realtime consumer must discard queued data after a state change.

### Callback interface (Observer-style)
- **Where:** `AudioCallback` in `src/engine/AudioDevice.h`; `Deck` implements it.
- **Why:** engine units plug into any device, real or mock.
- **Reuse when:** adding mixer/FX stages that process audio blocks.

### Lock-free metrics monitor
- **Where:** `src/engine/LatencyMonitor.h`
- **Why:** expose realtime stats (latency, xruns) to UI/tests without locking the audio thread.
- **Reuse when:** adding any realtime telemetry.

### Static library per area
- **Where:** `openmixer_engine` in `CMakeLists.txt`
- **Why:** shared, independently testable modules.
- **Reuse when:** every new `src/<area>/` (controllers, analysis, library, ui).
