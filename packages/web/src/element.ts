/**
 * `<zuri-player src="scene.zur">` custom element. Import this module once to register it:
 *
 *     import '@seedexr/zuri/element';
 *
 * Attributes: `src`, `artboard`, `play`, `fit`, `align`, `background`. Emits a `zuri-event`
 * CustomEvent (detail = event name) for each event the artboard reports. The underlying
 * player is available as `element.player` once the `load` event has fired.
 */
import { type Alignment, type Fit, ZuriPlayer } from './index.js';

export class ZuriPlayerElement extends HTMLElement {
  static observedAttributes = ['src', 'artboard', 'play', 'fit', 'align', 'background'];
  player?: ZuriPlayer;
  private canvas = document.createElement('canvas');
  private generation = 0;

  constructor() {
    super();
    const root = this.attachShadow({ mode: 'open' });
    root.innerHTML = '<style>:host{display:inline-block;width:300px;height:150px}canvas{width:100%;height:100%;display:block;touch-action:none}</style>';
    root.append(this.canvas);
  }

  connectedCallback(): void {
    void this.load();
  }
  disconnectedCallback(): void {
    this.generation++;
    this.player?.destroy();
    this.player = undefined;
  }
  attributeChangedCallback(): void {
    if (this.isConnected) void this.load();
  }

  private async load(): Promise<void> {
    const src = this.getAttribute('src');
    const generation = ++this.generation;
    this.player?.destroy();
    this.player = undefined;
    if (!src) return;
    try {
      const player = await ZuriPlayer.create(this.canvas, {
        src,
        artboard: this.getAttribute('artboard') ?? undefined,
        play: this.getAttribute('play') ?? undefined,
        fit: (this.getAttribute('fit') as Fit | null) ?? undefined,
        align: (this.getAttribute('align') as Alignment | null) ?? undefined,
        background: this.getAttribute('background') ?? undefined,
        onEvent: (name) => this.dispatchEvent(new CustomEvent('zuri-event', { detail: name, bubbles: true })),
      });
      // An attribute changed (or the element left the page) while this was loading.
      if (generation !== this.generation) return player.destroy();
      this.player = player;
      this.dispatchEvent(new Event('load'));
    } catch (error) {
      this.dispatchEvent(new CustomEvent('error', { detail: error }));
    }
  }
}

if (!customElements.get('zuri-player')) customElements.define('zuri-player', ZuriPlayerElement);

declare global {
  interface HTMLElementTagNameMap {
    'zuri-player': ZuriPlayerElement;
  }
}
