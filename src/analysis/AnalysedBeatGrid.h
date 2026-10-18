// Adapter: an immutable BeatGridModel snapshot exposed through the engine's BeatGrid interface,
// so Sync/Quantizer consume analysed or user-corrected grids. Only the header-only engine
// interface is used (no link to openmixer_engine). Thread-safe because it never changes.
#pragma once

#include "BeatGridModel.h"
#include "engine/BeatGrid.h"

#include <cmath>
#include <memory>

namespace openmixer::analysis
{

class AnalysedBeatGrid final : public openmixer::BeatGrid
{
public:
    explicit AnalysedBeatGrid (BeatGridModel snapshot) : model (std::move (snapshot)) {}

    double getBpm() const noexcept override { return model.getBpm(); }
    int getBeatsPerBar() const noexcept override { return model.getBeatsPerBar(); }
    double beatAtFrame (std::int64_t frame) const noexcept override
    {
        return model.beatAtFrame (static_cast<double> (frame));
    }
    std::int64_t frameAtBeat (double beat) const noexcept override
    {
        return static_cast<std::int64_t> (std::llround (model.frameAtBeat (beat)));
    }

    const BeatGridModel& getModel() const noexcept { return model; }

private:
    const BeatGridModel model;
};

// Null for an invalid model, so callers can hand the result straight to Sync::setGrid.
inline std::shared_ptr<const openmixer::BeatGrid> makeEngineGrid (const BeatGridModel& model)
{
    return model.isValid() ? std::make_shared<AnalysedBeatGrid> (model) : nullptr;
}

} // namespace openmixer::analysis
