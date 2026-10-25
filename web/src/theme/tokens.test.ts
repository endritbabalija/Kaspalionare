// WCAG 2.1 AA (spec 5, 8 "Quality"): the token file holds spec 5's table exactly, and every text and
// background pair the components use is at least 4.5:1, in both themes.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const css = readFileSync(path.join(__dirname, 'tokens.css'), 'utf8');

function block(selector: string): Record<string, string> {
  const start = css.indexOf(selector);
  const open = css.indexOf('{', start);
  const close = css.indexOf('}', open);
  const out: Record<string, string> = {};
  for (const m of css.slice(open + 1, close).matchAll(/--([a-z-]+):\s*(#[0-9a-f]{6})/gi)) {
    out[m[1] as string] = (m[2] as string).toLowerCase();
  }
  return out;
}

function lum(hex: string): number {
  const c = [1, 3, 5]
    .map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * (c[0] ?? 0) + 0.7152 * (c[1] ?? 0) + 0.0722 * (c[2] ?? 0);
}

function contrast(a: string, b: string): number {
  const x = Math.max(lum(a), lum(b));
  const y = Math.min(lum(a), lum(b));
  return (x + 0.05) / (y + 0.05);
}

// [text, background] as the components draw them.
const PAIRS: [string, string][] = [
  ['ink', 'ground'],
  ['ink', 'surface'],
  ['graphite', 'ground'],
  ['graphite', 'surface'],
  ['link', 'ground'],
  ['link', 'surface'],
  ['amber', 'ground'],
  ['amber', 'surface'],
  ['on-ink', 'ink'],
  ['on-teal', 'teal'],
  ['on-mint', 'mint'],
  ['status-open-fg', 'status-open-bg'],
  ['status-closing-fg', 'status-closing-bg'],
  ['status-drawn-fg', 'status-drawn-bg'],
  ['graphite', 'status-drawn-bg'],
  ['status-void-fg', 'surface'],
  ['status-void-fg', 'ground'],
  ['ink', 'rule-soft'],
];

const SPEC_TABLE: Record<'dark' | 'light', Record<string, string>> = {
  dark: {
    ground: '#0e1012',
    surface: '#16191c',
    ink: '#eceeea',
    'on-ink': '#0e1012',
    graphite: '#9aa1a8',
    rule: '#2a2e33',
    'rule-soft': '#22262a',
    'control-line': '#3a3f44',
    teal: '#0e7a70',
    link: '#5cc8b8',
    mint: '#74cdbf',
    amber: '#e59a4e',
    'status-open-bg': '#12332f',
    'status-open-fg': '#7fd8ca',
    'status-closing-bg': '#3a2a17',
    'status-closing-fg': '#f0b27a',
    'status-drawn-bg': '#24282c',
    'status-drawn-fg': '#eceeea',
  },
  light: {
    ground: '#f6f6f3',
    surface: '#ffffff',
    ink: '#15171a',
    'on-ink': '#ffffff',
    graphite: '#565c63',
    rule: '#dfe1dc',
    'rule-soft': '#ecedea',
    'control-line': '#c9ccc6',
    teal: '#0e7a70',
    link: '#0e7a70',
    mint: '#74cdbf',
    amber: '#a14f08',
    'status-open-bg': '#e3f3f0',
    'status-open-fg': '#0a5c54',
    'status-closing-bg': '#f3eadf',
    'status-closing-fg': '#7e3e06',
    'status-drawn-bg': '#e9eae6',
    'status-drawn-fg': '#15171a',
  },
};

describe.each([
  ['dark', block("[data-theme='dark']")],
  ['light', block("[data-theme='light']")],
] as const)('%s theme', (name, t) => {
  it('holds the spec 5 table values exactly', () => {
    for (const [k, v] of Object.entries(SPEC_TABLE[name])) expect(t[k], k).toBe(v);
  });
  it.each(PAIRS)('%s on %s is at least 4.5:1', (fg, bg) => {
    const a = t[fg];
    const b = t[bg];
    expect(a, fg).toBeDefined();
    expect(b, bg).toBeDefined();
    expect(contrast(a ?? '', b ?? '')).toBeGreaterThanOrEqual(4.5);
  });
});
