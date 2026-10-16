/*
  OpenMixer - binds mapper parameter ids to the Mixer and Decks.

  Ids:
    mixer.ch<N>.trim | fader | filter | pfl | xfader (0 thru, 1 left, 2 right)
    mixer.ch<N>.eq.low | eq.mid | eq.high            (dB)
    mixer.crossfader | mixer.master | mixer.cue.level | mixer.cue.mix
    deck.<N>.play (>=0.5 plays) | deck.<N>.rate | deck.<N>.seek (0..1 of length)
  Unknown ids are ignored and counted in getUnknownCount().
*/
#pragma once

#include "MidiMapper.h"
#include "../engine/Deck.h"
#include "../engine/Mixer.h"

#include <atomic>
#include <vector>

namespace openmixer
{

class EngineTarget : public ParameterTarget
{
public:
    EngineTarget (Mixer& m, std::vector<Deck*> d) : mixer (m), decks (std::move (d)) {}

    void setParameter (const juce::String& id, double value) override;
    int getUnknownCount() const noexcept { return unknown.load(); }

private:
    Mixer& mixer;
    std::vector<Deck*> decks;
    std::atomic<int> unknown { 0 };
};

} // namespace openmixer
