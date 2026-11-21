import { describe, expect, test } from 'vitest';
import { isRecordKey, takesTyping } from './keys';

const key = (init: { key: string } & Partial<Record<'repeat' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey', boolean>>) => ({
  repeat: false,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...init,
});

describe('the r key', () => {
  test('a bare r or R is the Record key', () => {
    expect(isRecordKey(key({ key: 'r' }))).toBe(true);
    expect(isRecordKey(key({ key: 'R' }))).toBe(true);
  });

  test('a modifier, a repeat or another key is not', () => {
    expect(isRecordKey(key({ key: 'r', ctrlKey: true }))).toBe(false);
    expect(isRecordKey(key({ key: 'r', metaKey: true }))).toBe(false);
    expect(isRecordKey(key({ key: 'r', altKey: true }))).toBe(false);
    expect(isRecordKey(key({ key: 'r', shiftKey: true }))).toBe(false);
    expect(isRecordKey(key({ key: 'r', repeat: true }))).toBe(false);
    expect(isRecordKey(key({ key: 'e' }))).toBe(false);
  });
});

describe('typing', () => {
  test('a text field, a select and a textarea take typed keys', () => {
    for (const tagName of ['INPUT', 'TEXTAREA', 'SELECT']) expect(takesTyping({ tagName })).toBe(true);
    expect(takesTyping({ tagName: 'DIV', isContentEditable: true })).toBe(true);
  });

  test('a button, the window and nothing do not', () => {
    expect(takesTyping({ tagName: 'BUTTON' })).toBe(false);
    expect(takesTyping({})).toBe(false);
    expect(takesTyping(null)).toBe(false);
  });
});
