import { readFileSync } from 'node:fs';
import { describe, expect, it, test } from 'vitest';
import { decodeSnapshot, firstSnapshot, packetLen, PACKET_MAGIC, tryDecode, type Snapshot, type StartStream } from './packet';

const golden = readFileSync(new URL('../../../../../fixtures/ui-bridge/snapshot-v1.bin', import.meta.url));
const buf = golden.buffer.slice(golden.byteOffset, golden.byteOffset + golden.byteLength) as ArrayBuffer;

test('decodes the golden packet', () => {
  const s = decodeSnapshot(buf, 1000);
  expect(s.decks).toHaveLength(2);
  expect(s.decks[0]).toMatchObject({ position: 441000, length: 11025000, rate: 1.02, fileRate: 44100, loaded: true, playing: true });
  expect(s.decks[1].loaded).toBe(false);
  expect(s.channels).toHaveLength(2);
  expect(s.channels[0]).toMatchObject({ left: 0.5, right: 0.25 });
  expect(s.channels[0].fader).toBeCloseTo(0.85);
  expect(s.channels[0].eq).toHaveLength(3);
  expect(s.master.left).toBeCloseTo(0.75);
  expect(s.master.crossfader).toBe(0.5);
  expect(s.ageMs).toBe(2.5);
  expect(s.ticks).toBe(7n);
  expect(s.receivedAt).toBe(1000);
});

test('the golden packet is 184 bytes and starts with the little-endian magic', () => {
  expect(packetLen(2, 2)).toBe(184);
  expect(buf.byteLength).toBe(184);
  expect(new Uint8Array(buf).slice(0, 4)).toEqual(new Uint8Array([0x31, 0x53, 0x4d, 0x4f]));
  expect(new DataView(buf).getUint32(0, true)).toBe(PACKET_MAGIC);
});

test('rejects a bad magic and a short buffer', () => {
  const bad = buf.slice(0);
  new DataView(bad).setUint32(0, 0, true);
  expect(() => decodeSnapshot(bad, 0)).toThrow('bad packet');
  expect(() => decodeSnapshot(buf.slice(0, 20), 0)).toThrow('bad packet');
});

test('rejects a wrong version', () => {
  const bad = buf.slice(0);
  new DataView(bad).setUint16(4, 2, true);
  expect(() => decodeSnapshot(bad, 0)).toThrow('bad packet');
});

test('rejects a buffer shorter than its own deck and channel counts imply', () => {
  expect(() => decodeSnapshot(buf.slice(0, 183), 0)).toThrow('bad packet');
  const more = buf.slice(0);
  new Uint8Array(more)[6] = 3;
  expect(() => decodeSnapshot(more, 0)).toThrow('bad packet');
});

test('tryDecode returns null instead of throwing', () => {
  expect(tryDecode(new ArrayBuffer(3), 0)).toBeNull();
  expect(tryDecode(buf, 0)?.decks).toHaveLength(2);
});

describe('firstSnapshot', () => {
  it('resolves with the first snapshot the stream delivers', async () => {
    const snap = decodeSnapshot(buf, 0);
    const start: StartStream = async (onSnapshot) => {
      onSnapshot(snap);
      onSnapshot(decodeSnapshot(buf, 1));
    };
    await expect(firstSnapshot(start)).resolves.toBe(snap);
  });

  it('rejects when the stream does not start', async () => {
    const start: StartStream = (_onSnapshot: (s: Snapshot) => void) => Promise.reject(new Error('no stream'));
    await expect(firstSnapshot(start)).rejects.toThrow('no stream');
  });
});
