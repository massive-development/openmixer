import type { StripInk } from '../ipc';
import { FRAGMENT_SHADER, MAX_TAPS, TEXTURE_WIDTH, VERTEX_SHADER } from './shader';
import type { LevelSize } from './view';

/** A deck's lane ink as the shader takes it: `#rrggbb` turned into 0..1 channels once. */
export interface LaneInk {
  well: Float32Array;
  body: Float32Array;
  highs: Float32Array;
  highsAlpha: number;
  shade: Float32Array;
  /** 0 when the style has no shade and dims by the played brightness instead. */
  shadeAlpha: number;
  gridBar: Float32Array;
  gridBeat: Float32Array;
  playhead: Float32Array;
  halo: Float32Array;
  /** CSS px; 0 when the style has no halo. */
  haloWidth: number;
  /** CSS px. */
  playheadWidth: number;
}

/** One deck's uploaded peaks: the texture and the first row of each level. */
export interface PeakTexture {
  texture: WebGLTexture;
  rows: Int32Array;
}

/** Everything one lane's draw needs; the caller keeps one per deck and refills it each frame. */
export interface LanePaint {
  /** Lane's bottom edge from the canvas bottom, and its size, in device px. */
  originY: number;
  width: number;
  height: number;
  centre: number;
  spp: number;
  length: number;
  peaks: PeakTexture | null;
  level: number;
  count: number;
  spPoint: number;
  beatLen: number;
  firstBeat: number;
  beatLines: boolean;
  /** Device px per CSS px, rounded, at least 1. */
  scale: number;
  ink: LaneInk;
  fill: number;
  playedBrightness: number;
}

const UNIFORMS = [
  'u_peaks', 'u_origin', 'u_size', 'u_centre', 'u_spp', 'u_length', 'u_hasPeaks', 'u_row', 'u_count',
  'u_spPoint', 'u_taps', 'u_fill', 'u_beatLen', 'u_firstBeat', 'u_beatLines', 'u_lineWidth', 'u_well',
  'u_body', 'u_highs', 'u_highsAlpha', 'u_shade', 'u_shadeAlpha', 'u_playedBrightness', 'u_gridBar',
  'u_gridBeat', 'u_playhead', 'u_playheadWidth', 'u_halo', 'u_haloWidth',
] as const;
type UniformName = (typeof UNIFORMS)[number];
type Uniforms = Record<UniformName, WebGLUniformLocation | null>;

/** Writes `#rrggbb` into `out[offset..offset + 3]` as 0..1 channels; other text leaves black. */
export function hexRgb(hex: string, out: Float32Array, offset = 0): Float32Array {
  const value = /^#[0-9a-f]{6}$/i.test(hex) ? parseInt(hex.slice(1), 16) : 0;
  out[offset] = ((value >> 16) & 0xff) / 255;
  out[offset + 1] = ((value >> 8) & 0xff) / 255;
  out[offset + 2] = (value & 0xff) / 255;
  return out;
}

/** The shader's form of a deck's `strip_ink`. */
export function laneInk(ink: StripInk): LaneInk {
  const rgb = (hex: string): Float32Array => hexRgb(hex, new Float32Array(3));
  const rgba = (hex: string, alpha: number): Float32Array => {
    const out = hexRgb(hex, new Float32Array(4));
    out[3] = alpha;
    return out;
  };
  return {
    well: rgb(ink.well),
    body: rgb(ink.body),
    highs: rgb(ink.highs),
    highsAlpha: ink.highsAlpha,
    shade: rgb(ink.playedShade ?? ink.well),
    shadeAlpha: ink.playedShade === null ? 0 : (ink.playedShadeAlpha ?? 0),
    gridBar: rgba(ink.gridBar, ink.gridBarAlpha),
    gridBeat: rgba(ink.gridBeat, ink.gridBeatAlpha),
    playhead: rgb(ink.playhead),
    halo: rgb(ink.playheadHalo ?? ink.well),
    haloWidth: ink.playheadHalo === null ? 0 : ink.playheadHaloWidth,
    playheadWidth: ink.playheadWidth,
  };
}

/** The first texture row of each level, and the rows they take together. */
export function levelRows(levels: readonly LevelSize[]): { rows: Int32Array; total: number } {
  const rows = new Int32Array(levels.length);
  let total = 0;
  levels.forEach((level, i) => {
    rows[i] = total;
    total += Math.ceil(level.len / TEXTURE_WIDTH);
  });
  return { rows, total };
}

/**
 * The texture bytes: each level's points (3 bytes each, levels back to back in `bytes`) from its
 * row; null when `bytes` does not hold exactly those levels (fetched across a track change).
 */
export function packRows(levels: readonly LevelSize[], bytes: Uint8Array, rows: Int32Array, total: number): Uint8Array | null {
  const points = levels.reduce((sum, level) => sum + level.len, 0);
  if (bytes.length !== points * 3) return null;
  const out = new Uint8Array(TEXTURE_WIDTH * total * 3);
  let from = 0;
  levels.forEach((level, i) => {
    out.set(bytes.subarray(from, from + level.len * 3), rows[i] * TEXTURE_WIDTH * 3);
    from += level.len * 3;
  });
  return out;
}

/** The WebGL2 side of the lanes: one program, one quad, one peak texture per deck. */
export class WaveGl {
  private readonly gl: WebGL2RenderingContext;
  private readonly u: Uniforms;
  private readonly blank: WebGLTexture;

  private constructor(gl: WebGL2RenderingContext, program: WebGLProgram) {
    this.gl = gl;
    this.u = uniformsOf(gl, program);
    this.blank = blankTexture(gl);
    gl.useProgram(program);
    gl.uniform1i(this.u.u_peaks, 0);
  }

  /** The lanes' GL over `canvas`; null (and logged) when WebGL2 or the shader is unavailable. */
  static create(canvas: HTMLCanvasElement): WaveGl | null {
    const gl = canvas.getContext('webgl2', { antialias: false, alpha: false, depth: false });
    if (!gl) {
      console.error('openmixer: no WebGL2 for the waveform');
      return null;
    }
    const program = linkProgram(gl);
    if (!program) return null;
    bindQuad(gl, program);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    return new WaveGl(gl, program);
  }

  /** Uploads a deck's peaks once; null when it has none or they do not fit a texture. */
  upload(levels: readonly LevelSize[], bytes: Uint8Array): PeakTexture | null {
    const { rows, total } = levelRows(levels);
    const gl = this.gl;
    if (total === 0 || total > gl.getParameter(gl.MAX_TEXTURE_SIZE)) return null;
    const data = packRows(levels, bytes, rows, total);
    if (!data) return null;
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB8, TEXTURE_WIDTH, total, 0, gl.RGB, gl.UNSIGNED_BYTE, data);
    nearest(gl);
    return { texture, rows };
  }

  /** Frees a deck's old peaks. */
  release(peaks: PeakTexture | null): void {
    if (peaks) this.gl.deleteTexture(peaks.texture);
  }

  /** Draws one lane into its strip of the canvas. */
  draw(lane: LanePaint): void {
    const gl = this.gl;
    gl.viewport(0, lane.originY, lane.width, lane.height);
    gl.bindTexture(gl.TEXTURE_2D, lane.peaks ? lane.peaks.texture : this.blank);
    this.setView(lane);
    this.setLevel(lane);
    this.setInk(lane);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  private setView(lane: LanePaint): void {
    const { gl, u } = this;
    gl.uniform2f(u.u_origin, 0, lane.originY);
    gl.uniform2f(u.u_size, lane.width, lane.height);
    gl.uniform1f(u.u_centre, lane.centre);
    gl.uniform1f(u.u_spp, lane.spp);
    gl.uniform1f(u.u_length, lane.length);
    gl.uniform1f(u.u_beatLen, lane.beatLen);
    gl.uniform1f(u.u_firstBeat, lane.firstBeat);
    gl.uniform1i(u.u_beatLines, lane.beatLines ? 1 : 0);
    gl.uniform1f(u.u_lineWidth, lane.scale);
  }

  private setLevel(lane: LanePaint): void {
    const { gl, u } = this;
    const has = lane.peaks !== null && lane.level >= 0 && lane.spPoint > 0;
    gl.uniform1i(u.u_hasPeaks, has ? 1 : 0);
    gl.uniform1i(u.u_row, has && lane.peaks ? lane.peaks.rows[lane.level] : 0);
    gl.uniform1i(u.u_count, has ? lane.count : 0);
    gl.uniform1f(u.u_spPoint, has ? lane.spPoint : 1);
    const taps = has ? Math.ceil(lane.spp / lane.spPoint) + 1 : 1;
    gl.uniform1i(u.u_taps, Math.min(Math.max(taps, 1), MAX_TAPS));
    gl.uniform1f(u.u_fill, lane.fill);
  }

  private setInk(lane: LanePaint): void {
    const { gl, u } = this;
    const ink = lane.ink;
    gl.uniform3fv(u.u_well, ink.well);
    gl.uniform3fv(u.u_body, ink.body);
    gl.uniform3fv(u.u_highs, ink.highs);
    gl.uniform1f(u.u_highsAlpha, ink.highsAlpha);
    gl.uniform3fv(u.u_shade, ink.shade);
    gl.uniform1f(u.u_shadeAlpha, ink.shadeAlpha);
    gl.uniform1f(u.u_playedBrightness, lane.playedBrightness);
    gl.uniform4fv(u.u_gridBar, ink.gridBar);
    gl.uniform4fv(u.u_gridBeat, ink.gridBeat);
    gl.uniform3fv(u.u_playhead, ink.playhead);
    gl.uniform1f(u.u_playheadWidth, Math.round(ink.playheadWidth * lane.scale));
    gl.uniform3fv(u.u_halo, ink.halo);
    gl.uniform1f(u.u_haloWidth, Math.round(ink.haloWidth * lane.scale));
  }
}

function uniformsOf(gl: WebGL2RenderingContext, program: WebGLProgram): Uniforms {
  const u = {} as Uniforms;
  for (const name of UNIFORMS) u[name] = gl.getUniformLocation(program, name);
  return u;
}

function compile(gl: WebGL2RenderingContext, type: GLenum, source: string): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (gl.getShaderParameter(shader, gl.COMPILE_STATUS)) return shader;
  console.error('openmixer: waveform shader', gl.getShaderInfoLog(shader));
  return null;
}

function linkProgram(gl: WebGL2RenderingContext): WebGLProgram | null {
  const vs = compile(gl, gl.VERTEX_SHADER, VERTEX_SHADER);
  const fs = compile(gl, gl.FRAGMENT_SHADER, FRAGMENT_SHADER);
  const program = gl.createProgram();
  if (!vs || !fs) return null;
  gl.attachShader(program, vs);
  gl.attachShader(program, fs);
  gl.linkProgram(program);
  if (gl.getProgramParameter(program, gl.LINK_STATUS)) return program;
  console.error('openmixer: waveform program', gl.getProgramInfoLog(program));
  return null;
}

function bindQuad(gl: WebGL2RenderingContext, program: WebGLProgram): void {
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
  const at = gl.getAttribLocation(program, 'a_corner');
  gl.enableVertexAttribArray(at);
  gl.vertexAttribPointer(at, 2, gl.FLOAT, false, 0, 0);
}

// Bound for a deck without peaks, so the sampler never reads an incomplete texture.
function blankTexture(gl: WebGL2RenderingContext): WebGLTexture {
  const texture = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB8, 1, 1, 0, gl.RGB, gl.UNSIGNED_BYTE, new Uint8Array(3));
  nearest(gl);
  return texture;
}

// No mipmaps: without these filters the texture is incomplete and samples as black.
function nearest(gl: WebGL2RenderingContext): void {
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
}
