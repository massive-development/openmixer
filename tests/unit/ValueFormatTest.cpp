// OpenMixer ValueFormatTest - control readouts
#include "ui/ValueFormat.h"

#include <gtest/gtest.h>

using namespace openmixer::ui;

TEST (ValueFormatTest, DecibelsCarryASignAndNeverShowNegativeZero)
{
    EXPECT_EQ (format::decibels (3.0), "+3.0 dB");
    EXPECT_EQ (format::decibels (-12.04), "-12.0 dB");
    EXPECT_EQ (format::decibels (0.0), "0.0 dB");
    EXPECT_EQ (format::decibels (-0.01), "0.0 dB");
}

TEST (ValueFormatTest, GainReadsAsDecibelsWithSilenceAsMinusInfinity)
{
    EXPECT_EQ (format::gainAsDecibels (1.0), "0.0 dB");
    EXPECT_EQ (format::gainAsDecibels (2.0), "+6.0 dB");
    EXPECT_EQ (format::gainAsDecibels (0.0), "-inf dB");
}

TEST (ValueFormatTest, FilterNamesTheSideOfTheSweep)
{
    EXPECT_EQ (format::filter (0.0), "Off");
    EXPECT_EQ (format::filter (0.001), "Off");
    EXPECT_EQ (format::filter (-0.5), "LPF 50%");
    EXPECT_EQ (format::filter (1.0), "HPF 100%");
}

TEST (ValueFormatTest, RateIsAPercentageAroundOriginalTempo)
{
    EXPECT_EQ (format::rate (1.0), "0.0%");
    EXPECT_EQ (format::rate (1.025), "+2.5%");
    EXPECT_EQ (format::rate (0.92), "-8.0%");
}
