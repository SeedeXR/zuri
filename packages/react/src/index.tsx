/**
 * React bindings for Zuri.
 *
 *     <Zuri src="/button.zur" style={{ width: 320, height: 160 }} onEvent={(name) => ...} />
 *
 * or, for access to the player (view-model properties, play / pause):
 *
 *     const { canvasRef, player } = useZuri({ src: '/button.zur' });
 *     useEffect(() => player?.instance.setColor('accent', color), [player, color]);
 *     return <canvas ref={canvasRef} style={{ width: 320, height: 160 }} />;
 *
 * Client-only: a canvas cannot render on the server. In Next.js put it in a "use client" file.
 */
import { useEffect, useMemo, useRef, useState, type CSSProperties, type RefCallback } from 'react';
import { ZuriPlayer, type PlayerOptions } from '@seedexr/zuri';

export interface UseZuri {
  /** Give this to the canvas: `<canvas ref={canvasRef} />`. The canvas may appear later. */
  canvasRef: RefCallback<HTMLCanvasElement>;
  /** The player once the file has loaded; null before that, while another is loading, and after unmount. */
  player: ZuriPlayer | null;
  /** Why loading failed, if it did. */
  error: Error | null;
}

/**
 * Create a player on a canvas. The player is created once per canvas and `src` /
 * `artboard` / `play` / `fit` / `align` / `background` (a `src` URL is compared by its
 * address, bytes by identity: keep the same array between renders); changing `onEvent`
 * does not recreate it.
 */
export function useZuri(options: PlayerOptions): UseZuri {
  const [canvas, canvasRef] = useState<HTMLCanvasElement | null>(null);
  const [made, setMade] = useState<{ player?: ZuriPlayer; error?: Error; of: object } | null>(null);
  const latest = useRef(options);
  latest.current = options;
  const src = options.src instanceof URL ? options.src.href : options.src;
  // A token for "this canvas and these options". What was made for an older one is
  // not handed out: for one render after a change, that player is already destroyed.
  const of = useMemo(() => ({}), [canvas, src, options.artboard, options.play, options.fit, options.align, options.background]);

  useEffect(() => {
    if (!canvas) return;
    let cancelled = false;
    let created: ZuriPlayer | undefined;
    ZuriPlayer.create(canvas, { ...latest.current, onEvent: (name) => latest.current.onEvent?.(name) }).then(
      (player) => {
        if (cancelled) return player.destroy();
        created = player;
        setMade({ player, of });
      },
      (e) => {
        if (!cancelled) setMade({ error: e instanceof Error ? e : new Error(String(e)), of });
      },
    );
    return () => {
      cancelled = true;
      created?.destroy();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `of` changes exactly when the canvas or an option the player is made from does
  }, [of]);

  const current = made?.of === of ? made : null;
  return { canvasRef, player: current?.player ?? null, error: current?.error ?? null };
}

export interface ZuriProps extends PlayerOptions {
  className?: string;
  style?: CSSProperties;
  /** Called once the file is loaded and playing. */
  onLoad?: (player: ZuriPlayer) => void;
  onError?: (error: Error) => void;
}

/** A canvas that plays a `.zur` file. Size it with CSS; it renders at device resolution. */
export function Zuri({ className, style, onLoad, onError, ...options }: ZuriProps) {
  const { canvasRef, player, error } = useZuri(options);
  const callbacks = useRef({ onLoad, onError });
  callbacks.current = { onLoad, onError };
  useEffect(() => {
    if (player) callbacks.current.onLoad?.(player);
  }, [player]);
  useEffect(() => {
    if (error) callbacks.current.onError?.(error);
  }, [error]);
  return <canvas ref={canvasRef} className={className} style={{ touchAction: 'none', ...style }} />;
}
