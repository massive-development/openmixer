#include "OrderedList.h"

#include <algorithm>
#include <limits>

namespace openmixer::library
{

int OrderedList::count (std::optional<std::int64_t> scopeId)
{
    auto s = db.prepare (("SELECT COUNT(*) FROM " + tableName + " WHERE " + scope + " IS ?1;").toRawUTF8());
    s.bindAll (scopeId);
    return s.nextRow() ? s.integer (0) : 0;
}

int OrderedList::insertIndex (std::optional<std::int64_t> scopeId, int requested)
{
    const auto n = count (scopeId);
    return requested < 0 ? n : std::min (requested, n);
}

int OrderedList::moveTarget (std::optional<std::int64_t> scopeId, int requested)
{
    return std::clamp (requested, 0, std::max (count (scopeId) - 1, 0));
}

juce::Result OrderedList::shift (std::optional<std::int64_t> scopeId, int delta, int from, int to)
{
    auto s = db.prepare (("UPDATE " + tableName + " SET position = position + ?2 WHERE " + scope
                          + " IS ?1 AND position BETWEEN ?3 AND ?4;").toRawUTF8());
    s.bindAll (scopeId, delta, from, to);
    return s.run();
}

juce::Result OrderedList::openGap (std::optional<std::int64_t> scopeId, int index)
{
    return shift (scopeId, 1, index, std::numeric_limits<int>::max());
}

juce::Result OrderedList::closeGap (std::optional<std::int64_t> scopeId, int position)
{
    return shift (scopeId, -1, position + 1, std::numeric_limits<int>::max());
}

juce::Result OrderedList::shiftForMove (std::optional<std::int64_t> scopeId, int oldPos, int newPos)
{
    if (oldPos < newPos)
        return shift (scopeId, -1, oldPos + 1, newPos);
    if (oldPos > newPos)
        return shift (scopeId, 1, newPos, oldPos - 1);
    return juce::Result::ok();
}

} // namespace openmixer::library
