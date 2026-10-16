// Mixer: channel strips, crossfader, master bus and PFL cue bus.
// Every control is also reachable through setParameter(), the shared MIDI/UI/test entry point.
#pragma once

#include "../core/ParameterTarget.h"
#include "ChannelStrip.h"
#include "Crossfader.h"

#include <array>

namespace openmixer
{

class Mixer : public ParameterTarget
{
public:
    static constexpr int kMaxChannels = 4;
    static constexpr float kMinEqDb = ThreeBandEq::kMinDb;
    static constexpr float kMaxEqDb = ThreeBandEq::kMaxDb;

    struct ChannelInput { const float* left = nullptr; const float* right = nullptr; };

    explicit Mixer (int numChannels = 2);

    int getNumChannels() const noexcept { return numChannels; }
    void prepare (double sampleRate, int maxBlockSize);

    // Ids: ch<N>.trim|fader|filter|pfl|xfader, ch<N>.eq.low|mid|high,
    //      crossfader, master, cue.level, cue.mix
    bool setParameter (const juce::String& id, double value) override;

    void setTrim (int ch, float linearGain) noexcept   { strip (ch).setTrim (linearGain); }
    void setFader (int ch, float v) noexcept           { strip (ch).setFader (v); }
    void setEq (int ch, int band, float dB) noexcept   { strip (ch).getEq().setGainDb (band, dB); }
    void setFilter (int ch, float v) noexcept          { strip (ch).getFilter().setPosition (v); }
    void setPfl (int ch, bool on) noexcept             { strip (ch).setPfl (on); }
    void setCrossfaderAssign (int ch, CrossfaderAssign a) noexcept { strip (ch).setAssign (a); }

    void setCrossfader (float position) noexcept       { crossfader.setPosition (position); }
    void setCrossfaderCurve (CrossfaderCurve c) noexcept { crossfader.setCurve (c); }
    void setMasterGain (float linearGain) noexcept;
    void setCueLevel (float linearGain) noexcept;
    void setCueMix (float mix) noexcept;     // 0 cue only .. 1 master only

    float getFader (int ch) const noexcept   { return strip (ch).getFader(); }
    float getFilter (int ch) const noexcept  { return strip (ch).getFilter().getPosition(); }
    float getEq (int ch, int band) const noexcept { return strip (ch).getEq().getGainDb (band); }
    bool getPfl (int ch) const noexcept      { return strip (ch).getPfl(); }
    float getChannelPeak (int ch) const noexcept { return strip (ch).getMeter().get(); }
    float getMasterPeakLeft() const noexcept  { return masterPeakL.get(); }
    float getMasterPeakRight() const noexcept { return masterPeakR.get(); }

    // Extension points: insert a stage into a channel or master chain at runtime.
    bool setChannelInsert (int ch, int slot, ProcessStage* stage) { return strip (ch).getInserts().set (slot, stage); }
    bool setMasterInsert (int slot, ProcessStage* stage) { return masterInserts.set (slot, stage); }

    // Audio thread. Outputs are overwritten.
    void process (const ChannelInput* inputs, float* masterL, float* masterR,
                  float* cueL, float* cueR, int numSamples) noexcept;

private:
    ChannelStrip& strip (int ch) noexcept;
    const ChannelStrip& strip (int ch) const noexcept;
    bool setChannelParameter (const juce::StringArray& parts, double value);
    void processChunk (const ChannelInput* inputs, const MixBus& bus, int n) noexcept;
    void finishMaster (const MixBus& bus, int n, float decay) noexcept;
    void mixHeadphones (const MixBus& bus, int n) noexcept;

    int numChannels;
    double sampleRate = 44100.0;
    int maxBlock = 1;
    std::array<ChannelStrip, kMaxChannels> channels;
    Crossfader crossfader;
    StageChain masterInserts;
    PeakMeter masterPeakL, masterPeakR;
    std::atomic<float> masterGain { 1.0F }, cueLevel { 1.0F }, cueMix { 0.0F };
};

} // namespace openmixer
