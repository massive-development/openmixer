/** Counts theme switches, so what reads the theme's ink once (the decks, the lanes) reads it again. */
export const themeEpoch = $state({ n: 0 });
