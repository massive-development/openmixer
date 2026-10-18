#include "ReverbFx.h"

namespace openmixer::fx
{

namespace
{
    // Mutually prime lengths (samples at 44.1 kHz), chosen for OpenMixer; right side is offset.
    constexpr std::array<int, ReverbFx::kNumCombs> kCombLengths { 1097, 1171, 1259, 1327, 1409, 1483, 1549, 1613 };
    constexpr std::array<int, ReverbFx::kNumAllpasses> kAllpassLengths { 563, 433, 337, 241 };
    constexpr int kStereoOffset = 29;
    constexpr float kInputGain = 0.02F;
    constexpr double kReferenceRate = 44100.0;
}

ReverbFx::ReverbFx() noexcept
    : Effect ({ { "size", 0.0F, 1.0F, 0.7F }, { "damp", 0.0F, 1.0F, 0.4F }, { "width", 0.0F, 1.0F, 1.0F } })
{
}

void ReverbFx::prepareEffect (double sampleRate, int)
{
    const double scale = sampleRate / kReferenceRate;
    for (size_t c = 0; c < 2; ++c)
    {
        const int offset = c == 0 ? 0 : kStereoOffset;
        for (size_t i = 0; i < combs[c].size(); ++i)
            combs[c][i].allocate (static_cast<int> ((kCombLengths[i] + offset) * scale));
        for (size_t i = 0; i < allpasses[c].size(); ++i)
            allpasses[c][i].allocate (static_cast<int> ((kAllpassLengths[i] + offset) * scale));
    }
}

void ReverbFx::reset() noexcept
{
    for (auto& side : combs)
        for (auto& comb : side)
            comb.reset();
    for (auto& side : allpasses)
        for (auto& ap : side)
            ap.reset();
}

void ReverbFx::beginReset() noexcept
{
    for (auto& side : combs)
        for (auto& comb : side)
            comb.beginClear();
    for (auto& side : allpasses)
        for (auto& ap : side)
            ap.beginClear();
}

bool ReverbFx::clearSome (int maxFrames) noexcept
{
    // One budget shared by all 24 buffers keeps the per-call cost bounded.
    bool clean = true;
    for (auto& side : combs)
        for (auto& comb : side)
            maxFrames -= comb.clearSome (maxFrames), clean = clean && comb.clean();
    for (auto& side : allpasses)
        for (auto& ap : side)
            maxFrames -= ap.clearSome (maxFrames), clean = clean && ap.clean();
    return clean;
}

int ReverbFx::tailSilenceWindow (const FxContext& ctx) const noexcept
{
    return static_cast<int> (ctx.sampleRate * 0.1);
}

float ReverbFx::processChannel (int channel, float in, float feedback, float damp) noexcept
{
    const auto c = static_cast<size_t> (channel);
    float sum = 0.0F;
    for (auto& comb : combs[c])
        sum += comb.process (in, feedback, damp);
    for (auto& ap : allpasses[c])
        sum = ap.process (sum);
    return sum;
}

void ReverbFx::render (float* left, float* right, int n, const FxContext&) noexcept
{
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        const float feedback = 0.7F + 0.27F * param (kSize), damp = 0.05F + 0.6F * param (kDamp);
        const float in = (left[i] + right[i]) * kInputGain;
        const float wl = processChannel (0, in, feedback, damp), wr = processChannel (1, in, feedback, damp);
        const float w = param (kWidth), mid = 0.5F * (wl + wr) * (1.0F - w);
        left[i] = wl * w + mid;
        right[i] = wr * w + mid;
    }
}

} // namespace openmixer::fx
