// Plugin-facing audio stage (versioned): insert into a channel or the master bus at runtime.
#pragma once

namespace openmixer
{

class ProcessStage
{
public:
    static constexpr int kInterfaceVersion = 1;

    virtual ~ProcessStage() = default;
    virtual int interfaceVersion() const noexcept { return kInterfaceVersion; }
    virtual void prepare (double /*sampleRate*/, int /*maxBlockSize*/) {}
    virtual void process (float* left, float* right, int numSamples) noexcept = 0;
};

} // namespace openmixer
