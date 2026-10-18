// OpenMixer MeterBallisticsTest - peak hold, release, clip latch and the MeterBar wrapper, with synthetic levels
#include <JuceHeader.h>
#include "ui/MeterBallistics.h"
#include "ui/MeterBar.h"

#include <gtest/gtest.h>

using namespace openmixer::ui;

namespace
{
MeterParams params()
{
    MeterParams p;
    p.floorDb = -48.0F;
    p.clipLevel = 0.99F;
    p.releaseSeconds = 0.5F;
    p.holdSeconds = 1.0F;
    p.clipSeconds = 2.0F;
    return p;
}
constexpr float kFrame = 1.0F / 30.0F;
}

TEST (MeterBallisticsTest, ProportionMapsDbOntoTheScale)
{
    EXPECT_FLOAT_EQ (MeterBallistics::proportionOf (0.0F, -48.0F), 0.0F);
    EXPECT_NEAR (MeterBallistics::proportionOf (1.0F, -48.0F), 1.0F, 1e-6F);
    EXPECT_NEAR (MeterBallistics::proportionOf (0.0630957F, -48.0F), 0.5F, 1e-3F);   // -24 dB is mid-scale
    EXPECT_FLOAT_EQ (MeterBallistics::proportionOf (1e-6F, -48.0F), 0.0F);             // below the floor
    EXPECT_FLOAT_EQ (MeterBallistics::proportionOf (4.0F, -48.0F), 1.0F);              // clamped
}

TEST (MeterBallisticsTest, AttackIsInstantAndReleaseIsExponential)
{
    MeterBallistics m (params());
    m.update (0.5F, kFrame);
    const float top = m.level();
    EXPECT_NEAR (top, MeterBallistics::proportionOf (0.5F, -48.0F), 1e-6F);

    m.update (0.0F, 0.5F);   // one release time constant
    EXPECT_NEAR (m.level(), top * std::exp (-1.0F), 1e-4F);
    for (int i = 0; i < 300; ++i)
        m.update (0.0F, kFrame);
    EXPECT_FLOAT_EQ (m.level(), 0.0F) << "snaps to zero instead of lingering";
}

TEST (MeterBallisticsTest, ReleaseIsFrameRateIndependent)
{
    MeterBallistics a (params()), b (params());
    a.update (0.5F, kFrame);
    b.update (0.5F, kFrame);
    a.update (0.0F, 0.2F);
    for (int i = 0; i < 4; ++i)
        b.update (0.0F, 0.05F);
    EXPECT_NEAR (a.level(), b.level(), 1e-4F);
}

TEST (MeterBallisticsTest, PeakHoldStaysThenDecays)
{
    MeterBallistics m (params());
    m.update (0.8F, kFrame);
    const float held = m.peakHold();
    EXPECT_GT (held, 0.9F);

    for (int i = 0; i < 25; ++i)   // 0.83 s < hold time
        m.update (0.0F, kFrame);
    EXPECT_FLOAT_EQ (m.peakHold(), held) << "still holding";
    EXPECT_LT (m.level(), held);

    for (int i = 0; i < 10; ++i)   // past the hold time
        m.update (0.0F, kFrame);
    EXPECT_LT (m.peakHold(), held) << "falls once the hold expires";

    m.update (0.9F, kFrame);   // a new higher peak re-arms the hold
    EXPECT_GT (m.peakHold(), held);
}

TEST (MeterBallisticsTest, LowerPeakDoesNotLowerTheHeldTick)
{
    MeterBallistics m (params());
    m.update (0.8F, kFrame);
    const float held = m.peakHold();
    m.update (0.2F, kFrame);
    EXPECT_FLOAT_EQ (m.peakHold(), held);
}

TEST (MeterBallisticsTest, ClipLatchesForItsHoldTimeAndCanBeCleared)
{
    MeterBallistics m (params());
    EXPECT_FALSE (m.isClipping());
    m.update (1.0F, kFrame);
    EXPECT_TRUE (m.isClipping());
    for (int i = 0; i < 30; ++i)   // 1 s later, silent: still lit
        m.update (0.0F, kFrame);
    EXPECT_TRUE (m.isClipping());
    for (int i = 0; i < 40; ++i)   // 2.3 s total: gone
        m.update (0.0F, kFrame);
    EXPECT_FALSE (m.isClipping());

    m.update (1.2F, kFrame);
    m.resetClip();
    EXPECT_FALSE (m.isClipping());
    m.update (0.5F, kFrame);
    EXPECT_FALSE (m.isClipping());
}

TEST (MeterBallisticsTest, UpdateReportsWhetherAnythingChanged)
{
    MeterBallistics m (params());
    EXPECT_FALSE (m.update (0.0F, kFrame)) << "silence on a silent meter needs no repaint";
    EXPECT_TRUE (m.update (0.5F, kFrame));
    EXPECT_TRUE (m.update (0.0F, kFrame));
}

TEST (MeterBallisticsTest, DefaultsComeFromTheTheme)
{
    const auto p = MeterParams::fromTheme();
    EXPECT_LT (p.floorDb, 0.0F);
    EXPECT_GT (p.releaseSeconds, 0.0F);
    EXPECT_GT (p.clipLevel, 0.5F);
}

TEST (MeterBallisticsTest, MeterBarFollowsUpdatesPaintsAndClearsClipOnClick)
{
    MeterBar bar;
    bar.setSize (10, 120);
    bar.update (1.0F, kFrame);
    EXPECT_TRUE (bar.getBallistics().isClipping());
    EXPECT_GT (bar.getBallistics().level(), 0.9F);
    const auto image = bar.createComponentSnapshot (bar.getLocalBounds());
    EXPECT_FALSE (image.isNull());

    const juce::MouseEvent click (juce::Desktop::getInstance().getMainMouseSource(), {}, {}, 0.0F, 0.0F, 0.0F, 0.0F, 0.0F,
                                  &bar, &bar, juce::Time::getCurrentTime(), {}, juce::Time::getCurrentTime(), 1, false);
    bar.mouseDown (click);
    EXPECT_FALSE (bar.getBallistics().isClipping());
}

TEST (MeterBallisticsTest, StereoMeterFeedsEachSideSeparately)
{
    StereoMeter meter;
    meter.setSize (22, 100);
    meter.update (0.5F, 0.0F, kFrame);
    EXPECT_GT (meter.getLeft().getBallistics().level(), 0.5F);
    EXPECT_FLOAT_EQ (meter.getRight().getBallistics().level(), 0.0F);
    EXPECT_FALSE (meter.getLeft().getBounds().intersects (meter.getRight().getBounds()));
}
