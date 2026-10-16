// OpenMixer TestMain - GoogleTest entry point with JUCE GUI/message-manager init
#include <JuceHeader.h>
#include <gtest/gtest.h>

int main (int argc, char** argv)
{
    ::testing::InitGoogleTest (&argc, argv);

    // Test discovery only lists names; skip slow GUI init so it fits CTest's timeout
    if (::testing::GTEST_FLAG (list_tests))
        return RUN_ALL_TESTS();

    juce::ScopedJuceInitialiser_GUI gui;
    return RUN_ALL_TESTS();
}
