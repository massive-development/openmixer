import { Channel, invoke } from '@tauri-apps/api/core';

/** The active theme as one `:root` block of `--om-*` variables. */
export const themeCss = (): Promise<string> => invoke<string>('theme_css');

/** Tells the shell the window has painted; a smoke run exits on it. */
export const ready = (): Promise<void> => invoke<void>('ready');

/** Whether this is a smoke run, which must see a snapshot packet before it calls `ready`. */
export const isSmoke = (): Promise<boolean> => invoke<boolean>('smoke');

/** Starts the 30 Hz snapshot stream (replacing any earlier one); each packet goes to `onPacket`. */
export function startStream(onPacket: (bytes: ArrayBuffer) => void): Promise<void> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = onPacket;
  return invoke<void>('start_stream', { channel });
}
