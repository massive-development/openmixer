#include "FileUrl.h"

#include <string>

namespace openmixer::library
{
namespace
{
int hexValue (char c) noexcept
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

// Not juce::URL::removeEscapeChars: it turns '+' into a space, which is valid in file names.
juce::String percentDecode (const juce::String& text)
{
    const std::string in = text.toStdString();
    std::string out;
    for (size_t i = 0; i < in.size(); ++i)
    {
        const int hi = (in[i] == '%' && i + 2 < in.size()) ? hexValue (in[i + 1]) : -1;
        const int lo = hi >= 0 ? hexValue (in[i + 2]) : -1;
        if (hi >= 0 && lo >= 0) { out.push_back (static_cast<char> (hi * 16 + lo)); i += 2; }
        else out.push_back (in[i]);
    }
    return juce::String::fromUTF8 (out.c_str(), static_cast<int> (out.size()));
}

bool hasDriveLetter (const juce::String& p) noexcept
{
    return p.length() >= 2 && juce::CharacterFunctions::isLetter (p[0]) && p[1] == ':';
}

juce::String nativeSeparators (const juce::String& p)
{
#if JUCE_WINDOWS
    return p.replaceCharacter ('/', '\\');
#else
    return p;
#endif
}
} // namespace

juce::String fileUrlToPath (const juce::String& url)
{
    const auto trimmed = url.trim();
    if (! trimmed.startsWithIgnoreCase ("file://"))
        return {};
    const auto rest = trimmed.substring (7);
    const int slash = rest.indexOfChar ('/');
    const auto host = slash < 0 ? rest : rest.substring (0, slash);
    auto path = percentDecode (slash < 0 ? juce::String() : rest.substring (slash));
    if (path.isEmpty())
        return {};
    if (host.isNotEmpty() && ! host.equalsIgnoreCase ("localhost"))
        path = "//" + host + path;                       // UNC share
    else if (hasDriveLetter (path.substring (1)))
        path = path.substring (1);                       // "/C:/x" -> "C:/x"
    return nativeSeparators (path);
}

} // namespace openmixer::library
