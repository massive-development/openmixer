/** log2 of the peak texture's row width. */
export const TEXTURE_SHIFT = 12;
/** Texels per row of the peak texture: every level is packed in rows this wide. */
export const TEXTURE_WIDTH = 1 << TEXTURE_SHIFT;
/** Most points of one level a pixel takes the maximum over (levels are about 4x apart). */
export const MAX_TAPS = 8;

/** A full-lane quad; the lane's place comes from the viewport. */
export const VERTEX_SHADER = `#version 300 es
in vec2 a_corner;
void main() {
  gl_Position = vec4(a_corner, 0.0, 1.0);
}
`;

/**
 * One lane of the Low-Band waveform (om-ui-gpui `paint_zoom`): the well, the body in the deck
 * colour and the high band over it, the beat grid and the playhead fixed at the centre. The played
 * part is not shaded: the centred playhead already splits the strip (comp 4a). Peaks are (body,
 * low, mid, high) heights as GPUI draws them. Positions are source frames, lengths device px.
 */
export const FRAGMENT_SHADER = `#version 300 es
precision highp float;
precision highp int;

uniform highp sampler2D u_peaks;
uniform vec2 u_origin;
uniform vec2 u_size;
uniform float u_centre;
uniform float u_spp;
uniform float u_length;
uniform bool u_hasPeaks;
uniform int u_row;
uniform int u_count;
uniform float u_spPoint;
uniform int u_taps;
uniform float u_fill;
uniform float u_minHalf;
uniform float u_beatLen;
uniform float u_firstBeat;
uniform bool u_beatLines;
uniform float u_lineWidth;
uniform vec3 u_well;
uniform vec3 u_body;
uniform vec3 u_highs;
uniform float u_highsAlpha;
uniform vec4 u_gridBar;
uniform vec4 u_gridBeat;
uniform vec3 u_playhead;
uniform float u_playheadWidth;
uniform vec3 u_halo;
uniform float u_haloWidth;

out vec4 outColour;

vec4 peak(int i) {
  if (i < 0 || i >= u_count) return vec4(0.0);
  return texelFetch(u_peaks, ivec2(i & ${TEXTURE_WIDTH - 1}, u_row + (i >> ${TEXTURE_SHIFT})), 0);
}

// Zoomed past the level: interpolate between point centres; otherwise the loudest covered point.
vec4 peaksAt(float frame) {
  if (u_spp < u_spPoint) {
    float f = frame / u_spPoint - 0.5;
    float i = floor(f);
    return mix(peak(int(i)), peak(int(i) + 1), f - i);
  }
  int first = int(floor((frame - 0.5 * u_spp) / u_spPoint));
  int last = int(floor((frame + 0.5 * u_spp) / u_spPoint));
  vec4 m = vec4(0.0);
  for (int k = 0; k < ${MAX_TAPS}; k++) {
    if (k >= u_taps || first + k > last) break;
    m = max(m, peak(first + k));
  }
  return m;
}

// GPUI's shape_px: a height above zero reaches at least u_minHalf px from the centre line.
float halfPx(float height) {
  return height > 0.0 ? max(height * 0.5 * u_size.y * u_fill, u_minHalf) : 0.0;
}

// Share of this pixel row inside a band reaching halfHeight px from the centre line.
float cover(float halfHeight, float d) {
  return halfHeight > 0.0 ? clamp(halfHeight - d + 0.5, 0.0, 1.0) : 0.0;
}

vec4 gridAt(float frame) {
  if (u_beatLen <= 0.0) return vec4(0.0);
  float beat = (frame - u_firstBeat) / u_beatLen;
  float k = floor(beat + 0.5);
  if (abs(beat - k) * u_beatLen / u_spp >= 0.5 * u_lineWidth) return vec4(0.0);
  if (mod(k, 4.0) < 0.5) return u_gridBar;
  return u_beatLines ? u_gridBeat : vec4(0.0);
}

vec3 wave(vec3 c, vec2 p, float frame) {
  vec4 b = peaksAt(frame);
  float d = abs(p.y - 0.5 * u_size.y);
  c = mix(c, u_body, cover(halfPx(b.r), d));
  return mix(c, u_highs, u_highsAlpha * cover(halfPx(b.a), d));
}

vec3 playhead(vec3 c, float x) {
  float left = floor(0.5 * u_size.x + 0.5) - floor(0.5 * u_playheadWidth + 0.5);
  float right = left + u_playheadWidth;
  if (x >= left && x < right) return u_playhead;
  if (u_haloWidth > 0.0 && x >= left - u_haloWidth && x < right + u_haloWidth) return u_halo;
  return c;
}

void main() {
  vec2 p = gl_FragCoord.xy - u_origin;
  float frame = u_centre + (p.x - 0.5 * u_size.x) * u_spp;
  vec3 c = u_well;
  if (u_spp > 0.0 && frame >= 0.0 && frame < u_length) {
    if (u_hasPeaks) c = wave(c, p, frame);
    vec4 g = gridAt(frame);
    c = mix(c, g.rgb, g.a);
  }
  outColour = vec4(playhead(c, p.x), 1.0);
}
`;
