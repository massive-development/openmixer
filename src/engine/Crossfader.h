// Crossfader position/curve and per-side gains.
#pragma once

#include <atomic>

namespace openmixer
{

enum class CrossfaderAssign { Thru = 0, Left = 1, Right = 2 };
enum class CrossfaderCurve { Linear = 0, ConstantPower = 1 };

struct CrossfaderGains { float left = 1.0F, right = 1.0F; };

class Crossfader
{
public:
    void setPosition (float p) noexcept;
    void setCurve (CrossfaderCurve c) noexcept { curve.store (c, std::memory_order_relaxed); }

    float getPosition() const noexcept { return position.load (std::memory_order_relaxed); }
    CrossfaderCurve getCurve() const noexcept { return curve.load (std::memory_order_relaxed); }
    CrossfaderGains gains() const noexcept;
    static float gainFor (const CrossfaderGains& g, CrossfaderAssign a) noexcept;

private:
    std::atomic<float> position { 0.5F };
    std::atomic<CrossfaderCurve> curve { CrossfaderCurve::ConstantPower };
};

} // namespace openmixer
