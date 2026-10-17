// Name -> effect creator registry; built-ins are pre-registered and plugins may add more.
// Control thread only.
#pragma once

#include "Effect.h"

#include <functional>
#include <map>
#include <memory>
#include <string>
#include <vector>

namespace openmixer::fx
{

class FxFactory
{
public:
    using Creator = std::function<std::unique_ptr<Effect>()>;

    // Factory with the eight built-in effects registered.
    static FxFactory withBuiltIns();
    // Built-in names in their default rack order.
    static std::vector<std::string> builtInNames();

    // Returns false if the name is empty, already taken or the creator is empty.
    bool registerEffect (const std::string& name, Creator creator);
    [[nodiscard]] std::unique_ptr<Effect> create (const std::string& name) const;
    std::vector<std::string> names() const;

private:
    std::map<std::string, Creator> creators;
};

} // namespace openmixer::fx
