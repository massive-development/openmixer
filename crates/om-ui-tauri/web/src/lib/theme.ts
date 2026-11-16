/** The id of the style element that carries the theme's `--om-*` variables. */
export const THEME_STYLE_ID = 'om-theme';

/** The parts of `Document` the theme touches, so tests can pass a stand-in. */
export type StyleHost = Pick<Document, 'getElementById' | 'createElement' | 'head'>;

/** Puts `css` in the theme style element, creating it once, so a theme change replaces it. */
export function applyThemeCss(host: StyleHost, css: string): HTMLElement {
  const style = host.getElementById(THEME_STYLE_ID) ?? host.head.appendChild(themeStyle(host));
  style.textContent = css;
  return style;
}

function themeStyle(host: StyleHost): HTMLStyleElement {
  return Object.assign(host.createElement('style'), { id: THEME_STYLE_ID });
}
