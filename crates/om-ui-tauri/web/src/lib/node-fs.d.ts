// Tests read the golden packet from disk; this is the one call they need, without `@types/node`.
declare module 'node:fs' {
  export function readFileSync(path: URL): Uint8Array;
}
