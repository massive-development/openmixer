#include "JuceMetadataReader.h"

namespace openmixer::library
{
namespace
{
// Tag keys differ per container (RIFF "IART" vs Vorbis "ARTIST"); match case-insensitively.
juce::String firstOf (const juce::StringPairArray& tags, std::initializer_list<const char*> keys)
{
    for (const auto* key : keys)
        for (const auto& k : tags.getAllKeys())
            if (k.equalsIgnoreCase (key) && tags[k].isNotEmpty())
                return tags[k].trim();
    return {};
}

int leadingYear (const juce::String& date)
{
    const auto year = date.substring (0, 4).getIntValue();
    return year >= 1000 && year <= 9999 ? year : 0;
}

void fillTags (const juce::StringPairArray& tags, TrackMetadata& out)
{
    using W = juce::WavAudioFormat;
    out.title = firstOf (tags, { W::riffInfoTitle, "title" });
    out.artist = firstOf (tags, { W::riffInfoArtist, "artist" });
    out.album = firstOf (tags, { W::riffInfoProductName, "album" });
    out.genre = firstOf (tags, { W::riffInfoGenre, "genre" });
    out.comment = firstOf (tags, { W::riffInfoComment2, "comment" });
    out.year = leadingYear (firstOf (tags, { W::riffInfoDateCreated, "date", "year" }));
}
} // namespace

JuceMetadataReader::JuceMetadataReader()
{
    formats.registerBasicFormats();
    for (int i = 0; i < formats.getNumKnownFormats(); ++i)
        for (const auto& e : formats.getKnownFormat (i)->getFileExtensions())
            exts.addIfNotAlreadyThere (e.trimCharactersAtStart (".").toLowerCase());
}

juce::Result JuceMetadataReader::read (const juce::File& file, TrackMetadata& out)
{
    std::unique_ptr<juce::AudioFormatReader> reader (formats.createReaderFor (file));
    if (reader == nullptr)
        return juce::Result::fail ("not a decodable audio file: " + file.getFileName());

    out = {};
    fillTags (reader->metadataValues, out);
    if (out.title.isEmpty())
        out.title = file.getFileNameWithoutExtension();   // why: library rows need something to show
    if (reader->sampleRate > 0.0)
        out.durationSec = static_cast<double> (reader->lengthInSamples) / reader->sampleRate;
    return juce::Result::ok();
}

} // namespace openmixer::library
