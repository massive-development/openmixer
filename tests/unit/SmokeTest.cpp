/*
  OpenMixer Unit Test Runner
  Main test registration point
*/

#include <JuceHeader.h>

namespace OpenMixerTests
{
    class SmokeTest : public juce::UnitTest
    {
    public:
        SmokeTest() : juce::UnitTest ("OpenMixer Smoke", "OpenMixer Core") {}

        void runTest() override
        {
            beginTest ("ProjectInfo is defined");
            juce::String name = ProjectInfo::projectName;
            expect (name.isNotEmpty(), "Project name should not be empty");
            juce::String version = ProjectInfo::versionString;
            expect (version.isNotEmpty(), "Project version should not be empty");
        }
    };

    SmokeTest smokeTest;
}

//==============================================================================
int main (int argc, char* argv[])
{
    juce::UnitTestRunner runner;
    runner.runAllTests();

    for (int i = 0; i < runner.getNumResults(); ++i)
        if (runner.getResult (i)->failures > 0)
            return 1;

    return 0;
}
