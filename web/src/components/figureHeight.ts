// The figure frame's one message (FigureFrame.tsx): { figure, height }, from the framed figure only.

export const FIGURE_HEIGHT = { initial: 620, min: 240, max: 3000 } as const;

/** The height a figure's message asks for, or null when the message is not this figure's. */
export function figureHeight(data: unknown, figure: string): number | null {
  if (typeof data !== 'object' || data === null) return null;
  const d = data as { figure?: unknown; height?: unknown };
  if (d.figure !== figure || typeof d.height !== 'number' || !Number.isFinite(d.height)) return null;
  return Math.min(FIGURE_HEIGHT.max, Math.max(FIGURE_HEIGHT.min, Math.round(d.height)));
}
