import { expect, test } from 'vitest';
import type { ControlSpec, Controls } from '../ipc';
import { cueWrites, sendInOrder } from './transport';

const trigger: ControlSpec = {
  id: '',
  min: 0,
  max: 1,
  default: 0,
  detent: null,
  kind: 'trigger',
  label: 'Cue',
  law: { kind: 'linear' },
  ticks: [],
  snap: null,
  writes: [],
};

// As the shell lists them: GPUI's Cue stops the deck, then returns it to the start.
const controls: Controls = {
  'deck1.cue': {
    ...trigger,
    writes: [
      { key: 'deck1.play', value: 0 },
      { key: 'deck1.seek', value: 0 },
    ],
  },
};

test("a deck's cue writes are the shell's, in its order", () => {
  expect(cueWrites(controls, 1)).toEqual([
    ['deck1.play', 0],
    ['deck1.seek', 0],
  ]);
});

test('a deck the shell lists no cue for writes nothing', () => {
  expect(cueWrites(controls, 0)).toEqual([]);
  expect(cueWrites({}, 1)).toEqual([]);
});

test('each write waits for the one before it', async () => {
  const log: string[] = [];
  let release: () => void = () => {};
  const first = new Promise<void>((resolve) => (release = resolve));
  const send = (key: string, value: number): Promise<void> => {
    log.push(`start ${key}=${value}`);
    return key.endsWith('play') ? first.then(() => void log.push('done play')) : Promise.resolve();
  };
  const done = sendInOrder(cueWrites(controls, 1), send);
  await Promise.resolve();
  expect(log).toEqual(['start deck1.play=0']);
  release();
  await done;
  expect(log).toEqual(['start deck1.play=0', 'done play', 'start deck1.seek=0']);
});

test('a failed write stops the ones after it', async () => {
  const sent: string[] = [];
  const send = (key: string): Promise<void> => {
    sent.push(key);
    return Promise.reject(new Error('refused'));
  };
  await expect(sendInOrder(cueWrites(controls, 1), send)).rejects.toThrow('refused');
  expect(sent).toEqual(['deck1.play']);
});
