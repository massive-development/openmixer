// StateReader over a DjEngine: the ids a controller map's feedback rules can use. Reads are atomics or a short
// uncontended mutex (sync), message thread only.
//   deck.<N>.play | loaded | keylock | loop | cue (loaded and stopped) | sync | sync.master | slip | quantize
//   deck.<N>.hotcue<K>.set (1 when the cue exists) | deck.<N>.rate | deck.<N>.position (0-1)
//   mixer.ch<N>.pfl | fader | trim | filter | level (decaying peak)
//   mixer.crossfader | mixer.master (gain) | mixer.master.level (decaying peak, louder channel)
#pragma once

#include "core/StateReader.h"
#include "engine/DjEngine.h"

namespace openmixer
{

class EngineStateReader : public StateReader
{
public:
    explicit EngineStateReader (DjEngine& e) noexcept : engine (e) {}

    bool readState (const juce::String& id, double& out) const override;

private:
    bool readDeck (const juce::StringArray& parts, double& out) const;
    bool readMixer (const juce::StringArray& parts, double& out) const;
    bool readChannel (int ch, const juce::String& what, double& out) const;

    DjEngine& engine;
};

} // namespace openmixer
