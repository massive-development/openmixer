#include "MidiFeedback.h"

namespace openmixer
{

MidiFeedback::MidiFeedback (const StateReader& s, MidiMapper& m, Clock c)
    : state (s), mapper (m), clock (std::move (c))
{
    if (! clock)
        clock = [] { return juce::Time::getMillisecondCounterHiRes(); };
}

void MidiFeedback::setSink (MidiOutputSink* newSink)
{
    sink = newSink;
    resend();
}

// A map load or edit changes the rules: take the new set and light everything again.
void MidiFeedback::reloadRules()
{
    seenGeneration = mapper.getGeneration();
    haveGeneration = true;
    rules = mapper.getOutputs();
    resend();
}

int MidiFeedback::update()
{
    if (sink == nullptr)
        return 0;
    if (! haveGeneration || seenGeneration != mapper.getGeneration())
        reloadRules();

    const double now = clock();
    int sent = 0;
    for (size_t i = 0; i < rules.size(); ++i)
    {
        double s = 0.0;
        if (! state.readState (rules[i].source, s))
            continue;   // unknown id (a plugin that is not loaded): leave the LED alone
        const int value = rules[i].evaluate (s, now);
        if (lastSent[i] == value)
            continue;
        sink->send (rules[i].toMessage (value));
        lastSent[i] = value;
        ++sent;
    }
    return sent;
}

} // namespace openmixer
