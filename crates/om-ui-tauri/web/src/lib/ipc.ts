import { invoke } from '@tauri-apps/api/core';

/** The active theme as one `:root` block of `--om-*` variables. */
export const themeCss = (): Promise<string> => invoke<string>('theme_css');

/** Tells the shell the window has painted; a smoke run exits on it. */
export const ready = (): Promise<void> => invoke<void>('ready');
