#include "FxFactory.h"

#include "CrushFx.h"
#include "EchoFx.h"
#include "FlangerFx.h"
#include "GateFx.h"
#include "PhaserFx.h"
#include "ReverbFx.h"
#include "RollFx.h"
#include "TapeStopFx.h"

namespace openmixer::fx
{

namespace
{
    template <typename T>
    FxFactory::Creator creatorFor()
    {
        return [] { return std::unique_ptr<Effect> (std::make_unique<T>()); };
    }
}

std::vector<std::string> FxFactory::builtInNames()
{
    return { "echo", "reverb", "flanger", "phaser", "crush", "gate", "roll", "tapestop" };
}

FxFactory FxFactory::withBuiltIns()
{
    FxFactory f;
    f.registerEffect ("echo", creatorFor<EchoFx>());
    f.registerEffect ("reverb", creatorFor<ReverbFx>());
    f.registerEffect ("flanger", creatorFor<FlangerFx>());
    f.registerEffect ("phaser", creatorFor<PhaserFx>());
    f.registerEffect ("crush", creatorFor<CrushFx>());
    f.registerEffect ("gate", creatorFor<GateFx>());
    f.registerEffect ("roll", creatorFor<RollFx>());
    f.registerEffect ("tapestop", creatorFor<TapeStopFx>());
    return f;
}

bool FxFactory::registerEffect (const std::string& name, Creator creator)
{
    if (name.empty() || ! creator)
        return false;
    return creators.emplace (name, std::move (creator)).second;
}

std::unique_ptr<Effect> FxFactory::create (const std::string& name) const
{
    const auto it = creators.find (name);
    return it == creators.end() ? nullptr : it->second();
}

std::vector<std::string> FxFactory::names() const
{
    std::vector<std::string> out;
    out.reserve (creators.size());
    for (const auto& entry : creators)
        out.push_back (entry.first);
    return out;
}

} // namespace openmixer::fx
