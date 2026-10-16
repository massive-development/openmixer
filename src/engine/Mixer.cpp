#include "Mixer.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace
{
    constexpr float kPi = 3.14159265358979f;
    constexpr float kEqLowHz = 250.0f, kEqMidHz = 1000.0f, kEqHighHz = 4000.0f;

    template <typename T> T clampv (T v, T lo, T hi) { return v < lo ? lo : (v > hi ? hi : v); }

    struct Coeffs { float b0, b1, b2, a1, a2; };

    Coeffs normalise (float b0, float b1, float b2, float a0, float a1, float a2)
    {
        return { b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0 };
    }

    Coeffs lowShelf (float sr, float f, float dB)
    {
        const float A = std::pow (10.0f, dB / 40.0f), w = 2 * kPi * f / sr;
        const float cs = std::cos (w), al = std::sin (w) / (2 * 0.70710678f), t = 2 * std::sqrt (A) * al;
        return normalise (A * ((A + 1) - (A - 1) * cs + t), 2 * A * ((A - 1) - (A + 1) * cs),
                          A * ((A + 1) - (A - 1) * cs - t),
                          (A + 1) + (A - 1) * cs + t, -2 * ((A - 1) + (A + 1) * cs), (A + 1) + (A - 1) * cs - t);
    }

    Coeffs highShelf (float sr, float f, float dB)
    {
        const float A = std::pow (10.0f, dB / 40.0f), w = 2 * kPi * f / sr;
        const float cs = std::cos (w), al = std::sin (w) / (2 * 0.70710678f), t = 2 * std::sqrt (A) * al;
        return normalise (A * ((A + 1) + (A - 1) * cs + t), -2 * A * ((A - 1) + (A + 1) * cs),
                          A * ((A + 1) + (A - 1) * cs - t),
                          (A + 1) - (A - 1) * cs + t, 2 * ((A - 1) - (A + 1) * cs), (A + 1) - (A - 1) * cs - t);
    }

    Coeffs peaking (float sr, float f, float dB, float q)
    {
        const float A = std::pow (10.0f, dB / 40.0f), w = 2 * kPi * f / sr;
        const float cs = std::cos (w), al = std::sin (w) / (2 * q);
        return normalise (1 + al * A, -2 * cs, 1 - al * A, 1 + al / A, -2 * cs, 1 - al / A);
    }

    Coeffs lowPass (float sr, float f, float q)
    {
        const float w = 2 * kPi * f / sr, cs = std::cos (w), al = std::sin (w) / (2 * q);
        return normalise ((1 - cs) / 2, 1 - cs, (1 - cs) / 2, 1 + al, -2 * cs, 1 - al);
    }

    Coeffs highPass (float sr, float f, float q)
    {
        const float w = 2 * kPi * f / sr, cs = std::cos (w), al = std::sin (w) / (2 * q);
        return normalise ((1 + cs) / 2, -(1 + cs), (1 + cs) / 2, 1 + al, -2 * cs, 1 - al);
    }
}

void Mixer::Biquad::run (int c, float* x, int n) noexcept
{
    float s1 = z1[c], s2 = z2[c];
    for (int i = 0; i < n; ++i)
    {
        const float in = x[i];
        const float out = b0 * in + s1;
        s1 = b1 * in - a1 * out + s2;
        s2 = b2 * in - a2 * out;
        x[i] = out;
    }
    z1[c] = s1;
    z2[c] = s2;
}

//==============================================================================
Mixer::Mixer (int n) : numChannels (clampv (n, 1, kMaxChannels)) {}

void Mixer::prepare (double sr, int maxBlockSize)
{
    sampleRate = sr > 0 ? sr : 44100.0;
    maxBlock = std::max (1, maxBlockSize);
    for (auto& s : scratch)
        s.assign ((size_t) maxBlock, 0.0f);
    for (auto& c : channels)
    {
        c.low.reset(); c.mid.reset(); c.high.reset(); c.sweep.reset();
        c.appliedEq[0] = c.appliedEq[1] = c.appliedEq[2] = 1000.0f;   // force coefficient refresh
        c.appliedFilter = 1000.0f;
        c.trimGain = c.trim.load();
        c.faderGain = -1.0f;   // sentinel: snap to target on first block
        c.peak.store (0.0f);
        updateCoefficients (c);
    }
}

#define OM_CH(ch) channels[(size_t) clampv (ch, 0, numChannels - 1)]

void Mixer::setTrim (int ch, float g) noexcept   { OM_CH (ch).trim.store (clampv (g, 0.0f, 4.0f)); }
void Mixer::setFader (int ch, float v) noexcept  { OM_CH (ch).fader.store (clampv (v, 0.0f, 1.0f)); }
void Mixer::setEq (int ch, int band, float dB) noexcept
{
    OM_CH (ch).eq[clampv (band, 0, 2)].store (clampv (dB, kMinEqDb, kMaxEqDb));
}
void Mixer::setFilter (int ch, float v) noexcept { OM_CH (ch).filter.store (clampv (v, -1.0f, 1.0f)); }
void Mixer::setPfl (int ch, bool on) noexcept    { OM_CH (ch).pfl.store (on); }
void Mixer::setCrossfaderAssign (int ch, CrossfaderAssign a) noexcept { OM_CH (ch).assign.store ((int) a); }

void Mixer::setCrossfader (float p) noexcept            { crossfader.store (clampv (p, 0.0f, 1.0f)); }
void Mixer::setCrossfaderCurve (CrossfaderCurve c) noexcept { curve.store ((int) c); }
void Mixer::setMasterGain (float g) noexcept            { masterGain.store (clampv (g, 0.0f, 2.0f)); }
void Mixer::setCueLevel (float g) noexcept              { cueLevel.store (clampv (g, 0.0f, 2.0f)); }
void Mixer::setCueMix (float m) noexcept                { cueMix.store (clampv (m, 0.0f, 1.0f)); }

float Mixer::getFader (int ch) const noexcept  { return OM_CH (ch).fader.load(); }
float Mixer::getFilter (int ch) const noexcept { return OM_CH (ch).filter.load(); }
float Mixer::getEq (int ch, int b) const noexcept { return OM_CH (ch).eq[clampv (b, 0, 2)].load(); }
bool Mixer::getPfl (int ch) const noexcept     { return OM_CH (ch).pfl.load(); }
float Mixer::getChannelPeak (int ch) const noexcept { return OM_CH (ch).peak.load(); }

//==============================================================================
void Mixer::updateCoefficients (Channel& c) noexcept
{
    const float sr = (float) sampleRate;
    const float eqNow[3] { c.eq[0].load(), c.eq[1].load(), c.eq[2].load() };

    auto apply = [] (Biquad& b, const Coeffs& k) { b.b0 = k.b0; b.b1 = k.b1; b.b2 = k.b2; b.a1 = k.a1; b.a2 = k.a2; };

    if (eqNow[0] != c.appliedEq[0]) apply (c.low,  lowShelf (sr, kEqLowHz, eqNow[0]));
    if (eqNow[1] != c.appliedEq[1]) apply (c.mid,  peaking (sr, kEqMidHz, eqNow[1], 0.7071f));
    if (eqNow[2] != c.appliedEq[2]) apply (c.high, highShelf (sr, kEqHighHz, eqNow[2]));
    for (int i = 0; i < 3; ++i) c.appliedEq[i] = eqNow[i];

    const float f = c.filter.load();
    if (f != c.appliedFilter)
    {
        c.appliedFilter = f;
        c.filterActive = std::abs (f) > 0.02f;
        if (c.filterActive)
        {
            const float maxHz = std::min (20000.0f, sr * 0.45f);
            if (f < 0)   // low-pass sweeping down from maxHz to 40 Hz
                apply (c.sweep, lowPass (sr, maxHz * std::pow (40.0f / maxHz, -f), 0.7071f));
            else         // high-pass sweeping up from 20 Hz to maxHz
                apply (c.sweep, highPass (sr, 20.0f * std::pow (maxHz / 20.0f, f), 0.7071f));
        }
    }
}

void Mixer::process (const ChannelInput* inputs, float* mL, float* mR, float* cL, float* cR, int numSamples) noexcept
{
    for (int done = 0; done < numSamples;)
    {
        const int n = std::min (maxBlock, numSamples - done);
        ChannelInput shifted[kMaxChannels];
        for (int c = 0; c < numChannels; ++c)
        {
            shifted[c].left  = inputs[c].left  ? inputs[c].left  + done : nullptr;
            shifted[c].right = inputs[c].right ? inputs[c].right + done : nullptr;
        }
        processChunk (shifted, mL + done, mR + done, cL + done, cR + done, n);
        done += n;
    }
}

void Mixer::processChunk (const ChannelInput* inputs, float* mL, float* mR, float* cL, float* cR, int n) noexcept
{
    std::fill (mL, mL + n, 0.0f); std::fill (mR, mR + n, 0.0f);
    std::fill (cL, cL + n, 0.0f); std::fill (cR, cR + n, 0.0f);

    const float pos = crossfader.load();
    float xl, xr;
    if (curve.load() == (int) CrossfaderCurve::Linear) { xl = 1.0f - pos; xr = pos; }
    else { xl = std::cos (pos * kPi * 0.5f); xr = std::sin (pos * kPi * 0.5f); }

    float* const bufs[2] { scratch[0].data(), scratch[1].data() };
    const float decay = std::exp (-(float) n / ((float) sampleRate * 0.3f));

    for (int ch = 0; ch < numChannels; ++ch)
    {
        auto& c = channels[(size_t) ch];
        const float* srcL = inputs[ch].left != nullptr ? inputs[ch].left : inputs[ch].right;
        const float* srcR = inputs[ch].right != nullptr ? inputs[ch].right : srcL;

        const float trimTarget = c.trim.load();
        const int a = c.assign.load();
        const float xf = a == (int) CrossfaderAssign::Left ? xl : (a == (int) CrossfaderAssign::Right ? xr : 1.0f);
        const float faderTarget = c.fader.load() * xf;
        if (c.faderGain < 0.0f) c.faderGain = faderTarget;

        if (srcL == nullptr)
        {
            c.trimGain = trimTarget; c.faderGain = faderTarget;
            c.peak.store (c.peak.load() * decay);
            continue;
        }

        updateCoefficients (c);

        const float tStep = (trimTarget - c.trimGain) / (float) n;
        for (int k = 0; k < 2; ++k)
        {
            const float* src = k == 0 ? srcL : srcR;
            float g = c.trimGain;
            for (int i = 0; i < n; ++i) { bufs[k][i] = src[i] * g; g += tStep; }
        }
        c.trimGain = trimTarget;

        for (int k = 0; k < 2; ++k)
        {
            c.low.run (k, bufs[k], n);
            c.mid.run (k, bufs[k], n);
            c.high.run (k, bufs[k], n);
            if (c.filterActive) c.sweep.run (k, bufs[k], n);
        }

        if (c.pfl.load (std::memory_order_relaxed))
            for (int i = 0; i < n; ++i) { cL[i] += bufs[0][i]; cR[i] += bufs[1][i]; }

        const float fStep = (faderTarget - c.faderGain) / (float) n;
        float g = c.faderGain, pk = 0.0f;
        for (int i = 0; i < n; ++i)
        {
            const float l = bufs[0][i] * g, r = bufs[1][i] * g;
            mL[i] += l; mR[i] += r;
            pk = std::max (pk, std::max (std::abs (l), std::abs (r)));
            g += fStep;
        }
        c.faderGain = faderTarget;
        c.peak.store (std::max (pk, c.peak.load() * decay));
    }

    const float mg = masterGain.load();
    float pkL = 0, pkR = 0;
    for (int i = 0; i < n; ++i)
    {
        mL[i] *= mg; mR[i] *= mg;
        pkL = std::max (pkL, std::abs (mL[i]));
        pkR = std::max (pkR, std::abs (mR[i]));
    }
    masterPeak[0].store (std::max (pkL, masterPeak[0].load() * decay));
    masterPeak[1].store (std::max (pkR, masterPeak[1].load() * decay));

    const float lvl = cueLevel.load(), mix = cueMix.load();
    for (int i = 0; i < n; ++i)
    {
        cL[i] = lvl * ((1.0f - mix) * cL[i] + mix * mL[i]);
        cR[i] = lvl * ((1.0f - mix) * cR[i] + mix * mR[i]);
    }
}

} // namespace openmixer
