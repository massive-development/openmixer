// Shared rig for the mixer widget tests: a real engine behind a real ParameterRouter, wrapped in a recorder.
#pragma once

#include <JuceHeader.h>
#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "ui/MixerBinding.h"

#include <algorithm>
#include <cmath>
#include <vector>

namespace mixertest
{

struct RecordingTarget : openmixer::ParameterTarget
{
    explicit RecordingTarget (openmixer::ParameterTarget& next) : forward (next) {}
    bool setParameter (const juce::String& id, double value) override
    {
        calls.emplace_back (id, value);
        return forward.setParameter (id, value);
    }
    bool saw (const juce::String& id, double value) const
    {
        return std::any_of (calls.begin(), calls.end(), [&] (const auto& c) { return c.first == id && std::abs (c.second - value) < 1e-9; });
    }
    bool sawId (const juce::String& id) const
    {
        return std::any_of (calls.begin(), calls.end(), [&] (const auto& c) { return c.first == id; });
    }
    openmixer::ParameterTarget& forward;
    std::vector<std::pair<juce::String, double>> calls;
};

struct Rig
{
    explicit Rig (int decks = 2) : engine (decks), recorder (router)
    {
        engine.prepare (48000.0, 256);
        router.registerTarget ("mixer", &engine.getMixer());
        router.registerTarget ("fx", &engine.getFx());
    }
    openmixer::ui::MixerBinding binding() { return { engine, recorder }; }

    // Runs one block with a sine on the left of channel 0 (right silent) so meters read something.
    void feedChannelZero (float amplitude)
    {
        std::vector<float> sine (256), silence (256, 0.0F), out[4];
        for (size_t i = 0; i < sine.size(); ++i)
            sine[i] = amplitude * std::sin (0.1F * static_cast<float> (i));
        for (auto& o : out)
            o.assign (256, 0.0F);
        openmixer::Mixer::ChannelInput in[openmixer::Mixer::kMaxChannels] {};
        in[0] = { sine.data(), silence.data() };
        engine.getMixer().process (in, out[0].data(), out[1].data(), out[2].data(), out[3].data(), 256);
    }

    openmixer::DjEngine engine;
    openmixer::ParameterRouter router;
    RecordingTarget recorder;
};

} // namespace mixertest
