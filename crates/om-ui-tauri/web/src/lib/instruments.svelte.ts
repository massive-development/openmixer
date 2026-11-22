import { onSamplerState, onStemsState, samplerState, stemsState, type SamplerState, type StemsState } from './ipc';

/** The sampler and each deck's stems as the shell last reported them. */
export class Instruments {
  // Raw: a state is replaced whole by each event, never mutated.
  sampler: SamplerState | null = $state.raw(null);
  stems: (StemsState | null)[] = $state.raw([]);

  /** Follows both for `decks` decks; resolves with the unlisten call. */
  async start(decks: number): Promise<() => void> {
    this.stems = Array.from({ length: decks }, () => null);
    const unsampler = await onSamplerState((s) => (this.sampler = s));
    const unstems = await onStemsState((s) => this.setStems(s));
    // Events only come on change, so the first state is asked for.
    samplerState().then((s) => (this.sampler ??= s), () => {});
    for (let deck = 0; deck < decks; deck++) {
      stemsState(deck).then((s) => s && !this.stems[deck] && this.setStems(s), () => {});
    }
    return () => {
      unsampler();
      unstems();
    };
  }

  private setStems(state: StemsState): void {
    if (state.deck < 0 || state.deck >= this.stems.length) return;
    this.stems = this.stems.map((s, i) => (i === state.deck ? state : s));
  }
}
