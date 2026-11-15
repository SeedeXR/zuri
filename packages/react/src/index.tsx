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
import { useEffect, useRef, useState, type CSSProperties, type RefObject } from 'react';
import { ZuriPlayer, type PlayerOptions } from '@seedexr/zuri';

export interface UseZuri {
  canvasRef: RefObject<HTMLCanvasElement | null>;
  /** The player once the file has loaded; null before that and after unmount. */
  player: ZuriPlayer | null;
  /** Why loading failed, if it did. */
  error: Error | null;
}

/**
 * Create a player on a canvas. The player is created once per `src` / `artboard` / `play` /
 * `fit` / `background`; changing `onEvent` does not recreate it.
 */
export function useZuri(options: PlayerOptions): UseZuri {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [player, setPlayer] = useState<ZuriPlayer | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const latest = useRef(options);
  latest.current = options;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let cancelled = false;
    let created: ZuriPlayer | undefined;
    setError(null);
    ZuriPlayer.create(canvas, { ...latest.current, onEvent: (name) => latest.current.onEvent?.(name) }).then(
      (p) => {
        if (cancelled) return p.destroy();
        created = p;
        setPlayer(p);
      },
      (e) => {
        if (!cancelled) setError(e instanceof Error ? e : new Error(String(e)));
      },
    );
    return () => {
      cancelled = true;
      created?.destroy();
      setPlayer(null);
    };
  }, [options.src, options.artboard, options.play, options.fit, options.background]);

  return { canvasRef, player, error };
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
