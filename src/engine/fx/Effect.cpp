#include "Effect.h"

#include <algorithm>

namespace openmixer::fx
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
}

Effect::Effect (std::initializer_list<FxParamSpec> list) noexcept
{
    for (const auto& spec : list)
    {
        if (numParams == kMaxParams)
            break;
        const auto i = static_cast<size_t> (numParams++);
        specs[i] = spec;
        targets[i].store (spec.defaultValue, kRelaxed);
    }
}

void Effect::prepare (double sampleRate, int maxBlockSize)
{
    for (size_t i = 0; i < static_cast<size_t> (numParams); ++i)
    {
        smoothers[i].setTime (sampleRate, specs[i].smoothSeconds);
        smoothers[i].snap (targets[i].load (kRelaxed));
    }
    prepareEffect (sampleRate, maxBlockSize);
    reset();
}

void Effect::process (float* left, float* right, int numSamples, const FxContext& ctx) noexcept
{
    for (size_t i = 0; i < static_cast<size_t> (numParams); ++i)
        blockTargets[i] = targets[i].load (kRelaxed);
    render (left, right, numSamples, ctx);
}

void Effect::tickParams() noexcept
{
    for (size_t i = 0; i < static_cast<size_t> (numParams); ++i)
        smoothers[i].step (blockTargets[i]);
}

const FxParamSpec& Effect::getParamSpec (int index) const noexcept
{
    return specs[static_cast<size_t> (std::clamp (index, 0, kMaxParams - 1))];
}

int Effect::findParam (std::string_view name) const noexcept
{
    for (int i = 0; i < numParams; ++i)
        if (specs[static_cast<size_t> (i)].name == name)
            return i;
    return -1;
}

bool Effect::setParam (int index, float value) noexcept
{
    if (index < 0 || index >= numParams || ! std::isfinite (value))
        return false;
    const auto& s = specs[static_cast<size_t> (index)];
    targets[static_cast<size_t> (index)].store (std::clamp (value, s.minValue, s.maxValue), kRelaxed);
    return true;
}

float Effect::getParamTarget (int index) const noexcept
{
    if (index < 0 || index >= numParams)
        return 0.0F;
    return targets[static_cast<size_t> (index)].load (kRelaxed);
}

} // namespace openmixer::fx
