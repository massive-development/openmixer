/** `OMS1` read little-endian: the first four bytes of every snapshot packet. */
export const PACKET_MAGIC = 0x4f4d5331;
/** The packet layout this decoder reads. */
export const PACKET_VERSION = 1;

const HEADER_LEN = 24;
const DECK_LEN = 40;
const CHANNEL_LEN = 32;
const MASTER_LEN = 16;

/** One deck's transport in a snapshot; positions and lengths are source frames. */
export interface DeckFrame {
  position: number;
  length: number;
  rate: number;
  fileRate: number;
  loaded: boolean;
  playing: boolean;
  looping: boolean;
  sync: boolean;
  keylock: boolean;
}

/** One mixer channel: its meter peaks and the controls it reports back. */
export interface ChannelFrame {
  left: number;
  right: number;
  fader: number;
  trim: number;
  filter: number;
  eq: [number, number, number];
}

/** The master meter and the controls that apply to every channel. */
export interface MasterFrame {
  left: number;
  right: number;
  gain: number;
  crossfader: number;
}

/** One decoded packet; `receivedAt` is on the clock the caller passes to `interpolate`. */
export interface Snapshot {
  ticks: bigint;
  ageMs: number;
  receivedAt: number;
  decks: DeckFrame[];
  channels: ChannelFrame[];
  master: MasterFrame;
}

/** Starts the stream, handing each decoded snapshot to `onSnapshot`. */
export type StartStream = (onSnapshot: (s: Snapshot) => void) => Promise<void>;

/** Bytes of a packet with `decks` decks and `channels` channels. */
export function packetLen(decks: number, channels: number): number {
  return HEADER_LEN + decks * DECK_LEN + channels * CHANNEL_LEN + MASTER_LEN;
}

/** Decodes one packet; throws `bad packet` on a wrong magic, a wrong version or a short buffer. */
export function decodeSnapshot(buf: ArrayBuffer, receivedAt: number): Snapshot {
  const view = new DataView(buf);
  if (!validHeader(view)) throw new Error('bad packet');
  const deckCount = view.getUint8(6);
  const channelCount = view.getUint8(7);
  if (buf.byteLength < packetLen(deckCount, channelCount)) throw new Error('bad packet');
  const channelsAt = HEADER_LEN + deckCount * DECK_LEN;
  return {
    ticks: view.getBigUint64(8, true),
    ageMs: view.getFloat32(16, true),
    receivedAt,
    decks: blocks(deckCount, (i) => readDeck(view, HEADER_LEN + i * DECK_LEN)),
    channels: blocks(channelCount, (i) => readChannel(view, channelsAt + i * CHANNEL_LEN)),
    master: readMaster(view, channelsAt + channelCount * CHANNEL_LEN),
  };
}

/** `decodeSnapshot`, or null for bytes that are not a packet. */
export function tryDecode(buf: ArrayBuffer, receivedAt: number): Snapshot | null {
  try {
    return decodeSnapshot(buf, receivedAt);
  } catch {
    return null;
  }
}

/** Starts the stream and resolves with its first snapshot; rejects when the start fails. */
export function firstSnapshot(start: StartStream): Promise<Snapshot> {
  return new Promise((resolve, reject) => {
    start(resolve).catch(reject);
  });
}

function validHeader(view: DataView): boolean {
  return (
    view.byteLength >= HEADER_LEN &&
    view.getUint32(0, true) === PACKET_MAGIC &&
    view.getUint16(4, true) === PACKET_VERSION
  );
}

function blocks<T>(count: number, read: (i: number) => T): T[] {
  return Array.from({ length: count }, (_, i) => read(i));
}

function readDeck(view: DataView, at: number): DeckFrame {
  const flags = view.getUint8(at + 32);
  const bit = (n: number): boolean => (flags & (1 << n)) !== 0;
  return {
    position: Number(view.getBigInt64(at, true)),
    length: Number(view.getBigInt64(at + 8, true)),
    rate: view.getFloat64(at + 16, true),
    fileRate: view.getFloat64(at + 24, true),
    loaded: bit(0),
    playing: bit(1),
    looping: bit(2),
    sync: bit(3),
    keylock: bit(4),
  };
}

function readChannel(view: DataView, at: number): ChannelFrame {
  const f = (i: number): number => view.getFloat32(at + i * 4, true);
  return { left: f(0), right: f(1), fader: f(2), trim: f(3), filter: f(4), eq: [f(5), f(6), f(7)] };
}

function readMaster(view: DataView, at: number): MasterFrame {
  const f = (i: number): number => view.getFloat32(at + i * 4, true);
  return { left: f(0), right: f(1), gain: f(2), crossfader: f(3) };
}
