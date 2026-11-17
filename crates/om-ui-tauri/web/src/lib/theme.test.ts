import { describe, expect, it } from 'vitest';
import { applyThemeCss, THEME_STYLE_ID, type StyleHost } from './theme';

interface FakeStyle {
  id: string;
  textContent: string | null;
}

function fakeHost(): { host: StyleHost; styles: FakeStyle[] } {
  const styles: FakeStyle[] = [];
  const host = {
    getElementById: (id: string) => styles.find((s) => s.id === id) ?? null,
    createElement: (): FakeStyle => ({ id: '', textContent: null }),
    head: {
      appendChild: (node: FakeStyle) => {
        styles.push(node);
        return node;
      },
    },
  };
  return { host: host as unknown as StyleHost, styles };
}

describe('applyThemeCss', () => {
  it('adds one style element carrying the theme', () => {
    const { host, styles } = fakeHost();
    applyThemeCss(host, ':root { --om-ground: #121211; }');
    expect(styles).toEqual([{ id: THEME_STYLE_ID, textContent: ':root { --om-ground: #121211; }' }]);
  });

  it('replaces the text on a theme change instead of stacking elements', () => {
    const { host, styles } = fakeHost();
    applyThemeCss(host, ':root { --om-ground: #121211; }');
    applyThemeCss(host, ':root { --om-ground: #f4f1ea; }');
    expect(styles).toHaveLength(1);
    expect(styles[0].textContent).toBe(':root { --om-ground: #f4f1ea; }');
  });
});
