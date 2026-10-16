// One mixer channel: trim, EQ, filter, inserts, PFL tap, fader/crossfader gain, meter.
#pragma once

#include "Crossfader.h"
#include "PeakMeter.h"
#include "StageChain.h"
#include "SweepFilter.h"
#include "ThreeBandEq.h"

#include <atomic>
#include <vector>

namespace openmixer
{

struct MixBus
{
    float* masterL = nullptr;
    float* masterR = nullptr;
    float* cueL = nullptr;
    float* cueR = nullptr;
};

class ChannelStrip
{
public:
    void prepare (double sampleRate, int maxBlockSize);

    void setTrim (float linear) noexcept;
    void setFader (float v) noexcept;
    void setPfl (bool on) noexcept { pfl.store (on, std::memory_order_relaxed); }
    void setAssign (CrossfaderAssign a) noexcept { assign.store (a, std::memory_order_relaxed); }

    float getFader() const noexcept { return fader.load (std::memory_order_relaxed); }
    bool getPfl() const noexcept { return pfl.load (std::memory_order_relaxed); }
    CrossfaderAssign getAssign() const noexcept { return assign.load (std::memory_order_relaxed); }

    ThreeBandEq& getEq() noexcept { return eq; }
    const ThreeBandEq& getEq() const noexcept { return eq; }
    SweepFilter& getFilter() noexcept { return filter; }
    const SweepFilter& getFilter() const noexcept { return filter; }
    StageChain& getInserts() noexcept { return inserts; }
    const PeakMeter& getMeter() const noexcept { return meter; }

    // Null srcL means silence; srcR defaults to srcL. Adds into the bus.
    void render (const float* srcL, const float* srcR, const CrossfaderGains& xf,
                 const MixBus& bus, int numSamples, float meterDecay) noexcept;

private:
    void applyTrim (const float* srcL, const float* srcR, int n) noexcept;
    float mixIntoMaster (float* const* buf, float targetGain, const MixBus& bus, int n) noexcept;

    std::atomic<float> trim { 1.0F }, fader { 1.0F };
    std::atomic<bool> pfl { false };
    std::atomic<CrossfaderAssign> assign { CrossfaderAssign::Thru };

    ThreeBandEq eq;
    SweepFilter filter;
    StageChain inserts;
    PeakMeter meter;

    float trimGain = 1.0F, faderGain = -1.0F;   // faderGain < 0: snap to target on first block
    std::vector<float> scratchL, scratchR;
};

} // namespace openmixer
