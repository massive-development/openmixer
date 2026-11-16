/** `OMS1` read little-endian: the first four bytes of every snapshot packet. */
export const PACKET_MAGIC = 0x4f4d5331;

/** Starts the stream, handing each packet to `onPacket`. */
export type StartStream = (onPacket: (bytes: ArrayBuffer) => void) => Promise<void>;

/** Whether `bytes` starts like a snapshot packet. */
export function isPacket(bytes: ArrayBuffer): boolean {
  return bytes.byteLength >= 4 && new DataView(bytes).getUint32(0, true) === PACKET_MAGIC;
}

/** Starts the stream and resolves with its first valid packet; rejects when the start fails. */
export function firstPacket(start: StartStream): Promise<ArrayBuffer> {
  return new Promise((resolve, reject) => {
    start((bytes) => {
      if (isPacket(bytes)) resolve(bytes);
    }).catch(reject);
  });
}
