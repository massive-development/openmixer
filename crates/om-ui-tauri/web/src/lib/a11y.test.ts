import { expect, test } from 'vitest';
import {
  clockText,
  controlName,
  deckName,
  hotcueName,
  loopName,
  meterText,
  Paced,
  positionText,
  valueText,
} from './a11y';

test('decks are named by letter', () => {
  expect(deckName(0)).toBe('Deck A');
  expect(deckName(1)).toBe('Deck B');
});

test('a control is named with its deck or channel and the word printed on it', () => {
  expect(controlName('ch0.trim', 'GAIN')).toBe('Channel A gain');
  expect(controlName('ch1.eq.high', 'HIGH')).toBe('Channel B high EQ');
  expect(controlName('ch0.eq.mid', 'MID')).toBe('Channel A mid EQ');
  expect(controlName('ch1.filter', 'FILTER')).toBe('Channel B filter');
  expect(controlName('ch0.fader', '')).toBe('Channel A fader');
  expect(controlName('mixer.master', 'MASTER')).toBe('Master gain');
  expect(controlName('mixer.crossfader', '')).toBe('Crossfader');
  expect(controlName('deck1.tempo', '')).toBe('Deck B tempo');
  expect(controlName('deck0.seek', '')).toBe('Deck A position');
  expect(controlName('deck0.play', 'Play')).toBe('Deck A play');
  expect(controlName('deck0.play', 'Pause')).toBe('Deck A pause');
  expect(controlName('deck1.sync', 'Sync')).toBe('Deck B sync');
});

test('an unknown key falls back to its label, then to the key', () => {
  expect(controlName('fx0.depth', 'DEPTH')).toBe('depth');
  expect(controlName('fx0.depth', '')).toBe('fx0.depth');
});

test('tempo reads as a signed percentage with a real minus sign', () => {
  expect(valueText('deck0.tempo', 1.02)).toBe('+2.00 %');
  expect(valueText('deck0.tempo', 0.965)).toBe('−3.50 %');
  expect(valueText('deck0.tempo', 1)).toBe('0.00 %');
  expect(valueText('deck0.tempo', 0.99999)).toBe('0.00 %');
});

test('gains read in decibels', () => {
  expect(valueText('ch0.trim', 1)).toBe('0.0 dB');
  expect(valueText('ch0.trim', 2)).toBe('+6.0 dB');
  expect(valueText('mixer.master', 0.5)).toBe('−6.0 dB');
  expect(valueText('ch0.trim', 0)).toBe('silent');
});

test('EQ bands read in decibels, the floor as a kill', () => {
  expect(valueText('ch0.eq.low', 0)).toBe('0.0 dB');
  expect(valueText('ch0.eq.low', -3.5)).toBe('−3.5 dB');
  expect(valueText('ch0.eq.high', 6)).toBe('+6.0 dB');
  expect(valueText('ch0.eq.mid', -48, { min: -48, max: 12 })).toBe('kill');
});

test('the filter names its side', () => {
  expect(valueText('ch0.filter', 0)).toBe('off');
  expect(valueText('ch0.filter', -0.4)).toBe('low-pass 40 %');
  expect(valueText('ch0.filter', 1)).toBe('high-pass 100 %');
});

test('a channel fader reads as a percentage of its travel', () => {
  expect(valueText('ch1.fader', 0.75)).toBe('75 %');
  expect(valueText('ch1.fader', 0)).toBe('0 %');
});

test('the crossfader names the deck it leans to', () => {
  expect(valueText('mixer.crossfader', 0.5)).toBe('centre');
  expect(valueText('mixer.crossfader', 0)).toBe('A 100 %');
  expect(valueText('mixer.crossfader', 0.75)).toBe('B 50 %');
});

test('an unknown key reads its number', () => {
  expect(valueText('fx0.depth', 0.25)).toBe('0.25');
});

test('clock text has no leading zero on the minutes', () => {
  expect(clockText(1.04)).toBe('0:01.0');
  expect(clockText(257.34)).toBe('4:17.3');
  expect(clockText(Number.NaN)).toBe('0:00.0');
});

test('a hot cue pad says whether it is set, and where', () => {
  expect(hotcueName(0, '3', 1.04)).toBe('Deck A hot cue 3, set at 0:01.0');
  expect(hotcueName(1, '6', null)).toBe('Deck B hot cue 6, empty');
});

test('a loop button names its length in beats', () => {
  expect(loopName(0, 0.25, '1/4')).toBe('Deck A loop 1/4 beat');
  expect(loopName(0, 1, '1')).toBe('Deck A loop 1 beat');
  expect(loopName(1, 4, '4')).toBe('Deck B loop 4 beats');
});

test('a level reads in whole decibels, silence below the floor', () => {
  expect(meterText(-12.4, -48)).toBe('−12 dB');
  expect(meterText(0, -48)).toBe('0 dB');
  expect(meterText(-48, -48)).toBe('silent');
  expect(meterText(-60, -48)).toBe('silent');
});

test('the track position reads as time of the whole', () => {
  expect(positionText(62.4, 124.9)).toBe('1:02 of 2:04');
  expect(positionText(0, 0)).toBe('no track');
});

test('a paced write passes at most once per period', () => {
  const paced = new Paced(250);
  expect(paced.due(1000)).toBe(true);
  expect(paced.due(1100)).toBe(false);
  expect(paced.due(1249)).toBe(false);
  expect(paced.due(1250)).toBe(true);
  expect(paced.due(1300)).toBe(false);
});
