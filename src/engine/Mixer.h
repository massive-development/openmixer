/*
  OpenMixer - Mixer: channel strips (trim, 3-band EQ, sweep filter, fader),
  crossfader, master bus and PFL cue (headphone) bus.

  Parameters are atomics and may be set from any thread (GUI, MIDI). process()
  runs on the audio thread and never allocates or locks; all scratch memory is
  allocated in prepare(). Filter coefficients use the public RBJ biquad
  formulas (original implementation).

  Signal flow per channel: trim -> EQ -> filter -> [PFL tap] -> fader*crossfader -> master.
  The PFL tap is pre-fader and ignores the crossfader.
  Headphone out = cueLevel * ((1 - cueMix) * cueBus + cueMix * master).
*/
#pragma once

#include <array>
#include <atomic>
#include <vector>

namespace openmixer
{

enum class CrossfaderAssign { Thru = 0, Left = 1, Right = 2 };
enum class CrossfaderCurve  { Linear = 0, ConstantPower = 1 };

class Mixer
{
public:
    static constexpr int kMaxChannels = 4;
    static constexpr float kMinEqDb = -48.0f;   // treated as "kill"
    static constexpr float kMaxEqDb = 6.0f;

    struct ChannelInput { const float* left = nullptr; const float* right = nullptr; };

    explicit Mixer (int numChannels = 2);

    int getNumChannels() const noexcept { return numChannels; }
    void prepare (double sampleRate, int maxBlockSize);

    // Parameters (any thread) ----------------------------------------------
    void setTrim (int ch, float linearGain) noexcept;          // 0..4
    void setFader (int ch, float v) noexcept;                  // 0..1
    void setEq (int ch, int band, float dB) noexcept;          // band 0=low 1=mid 2=high
    void setFilter (int ch, float v) noexcept;                 // -1 (LP) .. 0 (off) .. +1 (HP)
    void setPfl (int ch, bool on) noexcept;
    void setCrossfaderAssign (int ch, CrossfaderAssign a) noexcept;

    void setCrossfader (float position) noexcept;              // 0 (left) .. 1 (right)
    void setCrossfaderCurve (CrossfaderCurve c) noexcept;
    void setMasterGain (float linearGain) noexcept;            // 0..2
    void setCueLevel (float linearGain) noexcept;              // 0..2
    void setCueMix (float mix) noexcept;                       // 0 = cue only, 1 = master only

    float getFader (int ch) const noexcept;
    float getFilter (int ch) const noexcept;
    float getEq (int ch, int band) const noexcept;
    bool getPfl (int ch) const noexcept;

    // Meters (decaying peak, linear) ---------------------------------------
    float getChannelPeak (int ch) const noexcept;
    float getMasterPeakLeft() const noexcept  { return masterPeak[0].load(); }
    float getMasterPeakRight() const noexcept { return masterPeak[1].load(); }

    /** Audio thread. inputs: numChannels stereo sources (null pointers = silence).
        Writes master and cue stereo outputs (overwritten). */
    void process (const ChannelInput* inputs, float* masterL, float* masterR,
                  float* cueL, float* cueR, int numSamples) noexcept;

private:
    struct Biquad
    {
        float b0 = 1, b1 = 0, b2 = 0, a1 = 0, a2 = 0;
        float z1[2] { 0, 0 }, z2[2] { 0, 0 };
        void reset() noexcept { z1[0] = z1[1] = z2[0] = z2[1] = 0; }
        void run (int c, float* x, int n) noexcept;
    };

    struct Channel
    {
        std::atomic<float> trim { 1.0f }, fader { 1.0f }, eq[3] { { 0.0f }, { 0.0f }, { 0.0f } }, filter { 0.0f };
        std::atomic<bool> pfl { false };
        std::atomic<int> assign { (int) CrossfaderAssign::Thru };

        // audio-thread state
        Biquad low, mid, high, sweep;
        float appliedEq[3] { 0, 0, 0 };
        float appliedFilter = 0.0f;
        bool filterActive = false;
        float trimGain = 1.0f, faderGain = 1.0f;
        std::atomic<float> peak { 0.0f };
    };

    void updateCoefficients (Channel& c) noexcept;
    void processChunk (const ChannelInput* inputs, float* mL, float* mR, float* cL, float* cR, int n) noexcept;

    int numChannels;
    double sampleRate = 44100.0;
    int maxBlock = 0;
    std::array<Channel, kMaxChannels> channels;
    std::vector<float> scratch[2];

    std::atomic<float> crossfader { 0.5f }, masterGain { 1.0f }, cueLevel { 1.0f }, cueMix { 0.0f };
    std::atomic<int> curve { (int) CrossfaderCurve::ConstantPower };
    std::atomic<float> masterPeak[2] { { 0.0f }, { 0.0f } };
};

} // namespace openmixer
