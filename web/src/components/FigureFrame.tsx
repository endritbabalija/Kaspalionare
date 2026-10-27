// A figure in its own frame (How it works, D138): the figure is a separate file (web/public/figures/, built by
// docs/figures/<name>/build.mjs) with its own CSP (vercel.json "/figures/(.*)": its inline script by hash, framed by
// this site only). The frame is sandboxed with scripts only: the figure runs in an opaque origin, so it cannot read
// this page, its storage or the quick wallet's key. Its height comes from the figure's one message, accepted from
// this frame only, as a bounded number. States: loading (the default height), sized.
import { useEffect, useRef, useState } from 'react';
import { FIGURE_HEIGHT, figureHeight } from './figureHeight';

export interface FigureFrameProps {
  /** The figure file, e.g. "/figures/draw-cabinet-tour.html". */
  src: string;
  /** The figure's name in its message ({ figure, height }). */
  figure: string;
  title: string;
  testId: string;
}

export function FigureFrame({ src, figure, title, testId }: FigureFrameProps) {
  const ref = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState<number | null>(null);
  useEffect(() => {
    const on = (e: MessageEvent) => {
      if (!ref.current || e.source !== ref.current.contentWindow) return;
      const h = figureHeight(e.data, figure);
      if (h !== null) setHeight(h);
    };
    window.addEventListener('message', on);
    return () => window.removeEventListener('message', on);
  }, [figure]);
  return (
    <iframe
      ref={ref}
      src={src}
      title={title}
      data-testid={testId}
      data-state={height === null ? 'loading' : 'sized'}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
      loading="lazy"
      className="block w-full rounded-[14px] border-0 bg-ground"
      style={{ height: height ?? FIGURE_HEIGHT.initial }}
    />
  );
}
