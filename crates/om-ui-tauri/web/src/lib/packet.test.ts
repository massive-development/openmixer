import { describe, expect, it } from 'vitest';
import { firstPacket, isPacket, PACKET_MAGIC, type StartStream } from './packet';

function packet(magic: number): ArrayBuffer {
  const bytes = new ArrayBuffer(8);
  new DataView(bytes).setUint32(0, magic, true);
  return bytes;
}

describe('isPacket', () => {
  it('accepts the little-endian magic', () => {
    expect(new Uint8Array(packet(PACKET_MAGIC)).slice(0, 4)).toEqual(new Uint8Array([0x31, 0x53, 0x4d, 0x4f]));
    expect(isPacket(packet(PACKET_MAGIC))).toBe(true);
  });

  it('rejects other bytes and short buffers', () => {
    expect(isPacket(packet(0x314d534f))).toBe(false);
    expect(isPacket(new ArrayBuffer(3))).toBe(false);
  });
});

describe('firstPacket', () => {
  it('skips bytes that are not a packet', async () => {
    const good = packet(PACKET_MAGIC);
    const start: StartStream = async (onPacket) => {
      onPacket(packet(0));
      onPacket(good);
    };
    await expect(firstPacket(start)).resolves.toBe(good);
  });

  it('rejects when the stream does not start', async () => {
    const start: StartStream = () => Promise.reject(new Error('no stream'));
    await expect(firstPacket(start)).rejects.toThrow('no stream');
  });
});
