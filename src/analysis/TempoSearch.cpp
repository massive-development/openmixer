#include "TempoSearch.h"

#include <cmath>

namespace openmixer::analysis
{
namespace
{
double autocorrelation (const std::vector<float>& v, size_t lag)
{
    if (lag >= v.size())
        return 0.0;
    double sum = 0.0;
    for (size_t i = 0; i + lag < v.size(); ++i)
        sum += static_cast<double> (v[i]) * v[i + lag];
    return sum / static_cast<double> (v.size() - lag);
}

// Rewards lags whose double is also periodic, which suppresses off-beat (1.5x) peaks.
double lagScore (const std::vector<float>& v, size_t lag)
{
    return autocorrelation (v, lag) + 0.5 * autocorrelation (v, 2 * lag);
}

double parabolicOffset (double left, double centre, double right) noexcept
{
    const auto denom = left - 2.0 * centre + right;
    return std::abs (denom) > 1.0e-12 ? juce::jlimit (-0.5, 0.5, 0.5 * (left - right) / denom) : 0.0;
}

double beatScore (const OnsetEnvelope& env, double period, double phase)
{
    const auto end = static_cast<double> (env.values.size());
    double sum = 0.0;
    int count = 0;
    for (double idx = phase; idx < end; idx += period, ++count)
        sum += env.at (idx);
    return count > 0 ? sum / count : 0.0;
}

struct SearchBox { double bpmLo, bpmHi, bpmStep, phaseLo, phaseHi, phaseStep; };

BeatPhase searchBox (const OnsetEnvelope& env, const SearchBox& box)
{
    BeatPhase best;
    for (double bpm = box.bpmLo; bpm <= box.bpmHi; bpm += box.bpmStep)
    {
        const auto period = periodFrames (env, bpm);
        const auto phaseHi = std::min (box.phaseHi, box.phaseLo + period);
        for (double phase = box.phaseLo; phase < phaseHi; phase += box.phaseStep)
        {
            const auto score = beatScore (env, period, phase);
            if (score > best.score)
                best = { bpm, phase, score };
        }
    }
    return best;
}

double wrapPhase (double phase, double period) noexcept
{
    return phase - period * std::floor (phase / period);
}

// Interpolated position of the strongest envelope peak within +-radius of index; nullopt if flat.
std::optional<double> localPeak (const OnsetEnvelope& env, double index, double radius)
{
    const auto lo = static_cast<long> (std::ceil (std::max (1.0, index - radius)));
    const auto hi = static_cast<long> (std::floor (std::min (static_cast<double> (env.values.size()) - 2.0, index + radius)));
    long best = -1;
    for (auto i = lo; i <= hi; ++i)
        if (best < 0 || env.values[static_cast<size_t> (i)] > env.values[static_cast<size_t> (best)])
            best = i;
    if (best < 0 || env.values[static_cast<size_t> (best)] <= 0.0f)
        return std::nullopt;
    const auto b = static_cast<size_t> (best);
    return static_cast<double> (best) + parabolicOffset (env.values[b - 1], env.values[b], env.values[b + 1]);
}

struct LineFit { double slope = 0.0, intercept = 0.0; int count = 0; };

// Least-squares fit of onset peak index against beat number.
LineFit fitBeatLine (const OnsetEnvelope& env, const BeatPhase& beats, double period)
{
    double n = 0.0, sx = 0.0, sy = 0.0, sxx = 0.0, sxy = 0.0;
    int k = 0;
    for (double idx = beats.phase; idx < static_cast<double> (env.values.size()); idx += period, ++k)
        if (const auto peak = localPeak (env, idx, period * 0.15))
        {
            const auto x = static_cast<double> (k);
            n += 1.0; sx += x; sy += *peak; sxx += x * x; sxy += x * *peak;
        }
    const auto denom = n * sxx - sx * sx;
    if (n < 4.0 || std::abs (denom) < 1.0e-9)
        return {};
    const auto slope = (n * sxy - sx * sy) / denom;
    return { slope, (sy - slope * sx) / n, static_cast<int> (n) };
}
} // namespace

double periodFrames (const OnsetEnvelope& env, double bpm) noexcept
{
    return env.frameRate * 60.0 / bpm;
}

double estimateCoarseBpm (const OnsetEnvelope& env, double minBpm, double maxBpm)
{
    const auto lagLo = static_cast<size_t> (std::floor (periodFrames (env, maxBpm)));
    const auto lagHi = static_cast<size_t> (std::ceil (periodFrames (env, minBpm)));
    if (lagLo < 2 || lagHi * 2 >= env.values.size())
        return 0.0;

    size_t bestLag = 0;
    double bestScore = 0.0;
    for (auto lag = lagLo; lag <= lagHi; ++lag)
        if (const auto s = lagScore (env.values, lag); s > bestScore)
            std::tie (bestLag, bestScore) = std::make_pair (lag, s);
    if (bestLag == 0)
        return 0.0;

    const auto lag = static_cast<double> (bestLag)
                     + parabolicOffset (lagScore (env.values, bestLag - 1), bestScore, lagScore (env.values, bestLag + 1));
    return juce::jlimit (minBpm, maxBpm, env.frameRate * 60.0 / lag);
}

BeatPhase refineBeatPhase (const OnsetEnvelope& env, double coarseBpm)
{
    if (coarseBpm <= 0.0)
        return {};
    const auto period = periodFrames (env, coarseBpm);
    auto best = searchBox (env, { coarseBpm * 0.98, coarseBpm * 1.02, 0.05, 0.0, period, 0.5 });
    best = searchBox (env, { best.bpm - 0.06, best.bpm + 0.06, 0.002, best.phase - 1.0, best.phase + 1.0, 0.05 });
    best.phase = wrapPhase (best.phase, periodFrames (env, best.bpm));
    return fitToOnsets (env, best);
}

BeatPhase fitToOnsets (const OnsetEnvelope& env, const BeatPhase& beats)
{
    const auto period = periodFrames (env, beats.bpm);
    const auto fit = fitBeatLine (env, beats, period);
    if (fit.count == 0 || std::abs (fit.slope / period - 1.0) > 0.01)
        return beats;   // the line fit is only trusted as a small correction
    return { env.frameRate * 60.0 / fit.slope, wrapPhase (fit.intercept, fit.slope), beats.score };
}

int findDownbeatOffset (const OnsetEnvelope& lowBand, const BeatPhase& beats, int beatsPerBar)
{
    if (beatsPerBar <= 1 || beats.bpm <= 0.0)
        return 0;
    const auto period = periodFrames (lowBand, beats.bpm);
    std::vector<double> sums (static_cast<size_t> (beatsPerBar), 0.0);
    int beat = 0;
    for (double idx = beats.phase; idx < static_cast<double> (lowBand.values.size()); idx += period, ++beat)
        sums[static_cast<size_t> (beat % beatsPerBar)] += std::max ({ lowBand.at (idx - 1.0), lowBand.at (idx), lowBand.at (idx + 1.0) });
    return static_cast<int> (std::max_element (sums.begin(), sums.end()) - sums.begin());
}

} // namespace openmixer::analysis
