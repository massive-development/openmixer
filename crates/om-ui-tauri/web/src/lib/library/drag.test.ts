import { expect, test } from 'vitest';
import { accepts, isDrag, parseDrop, type Payload } from './drag';

const track: Payload = { kind: 'track', trackId: 7, row: 2, label: 'Song' };
const playlist: Payload = { kind: 'playlist', id: 4, label: 'Warm up' };

test('drop values name their targets', () => {
  expect(parseDrop('playlist:5')).toEqual({ kind: 'playlist', id: 5 });
  expect(parseDrop('folder:3')).toEqual({ kind: 'folder', id: 3 });
  expect(parseDrop('playlists')).toEqual({ kind: 'folder', id: null });
  expect(parseDrop('row:12')).toEqual({ kind: 'row', row: 12 });
});

test('anything else names no target', () => {
  expect(parseDrop(null)).toBeNull();
  expect(parseDrop('playlist:')).toBeNull();
  expect(parseDrop('playlist:x')).toBeNull();
  expect(parseDrop('deck:1')).toBeNull();
});

test('a track goes to a deck, a playlist or a row, never into a folder', () => {
  expect(accepts(track, { kind: 'deck', deck: 0 })).toBe(true);
  expect(accepts(track, { kind: 'playlist', id: 1 })).toBe(true);
  expect(accepts(track, { kind: 'row', row: 3 })).toBe(true);
  expect(accepts(track, { kind: 'folder', id: 1 })).toBe(false);
});

test('a playlist goes into a folder or the top level, not onto itself or a deck', () => {
  expect(accepts(playlist, { kind: 'folder', id: 9 })).toBe(true);
  expect(accepts(playlist, { kind: 'folder', id: null })).toBe(true);
  expect(accepts(playlist, { kind: 'folder', id: 4 })).toBe(false);
  expect(accepts(playlist, { kind: 'deck', deck: 0 })).toBe(false);
});

test('a press becomes a drag only after it travels', () => {
  expect(isDrag(1, 2)).toBe(false);
  expect(isDrag(4, 4)).toBe(true);
});
