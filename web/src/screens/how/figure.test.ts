// How it works (F6 part B, D138): the chain from the Rust lottery code to the served figure.
//   - cabinet-model.json is the lottery code's (crates/lottery/tests/f6_figure.rs `figure_model_is_current`);
//   - each embedded figure (public/figures/) was built from this very model (its sha256 is in the figure);
//   - vercel.json's figure CSP lists the hashes of each figure's inline script and style elements, frames them from
//     this site only, and the site's own rule keeps refusing every frame; the two rules never overlap;
//   - the figure's colours (docs/figures/draw-cabinet/embed.css) are the page's tokens (src/theme/tokens.css).
// After a deliberate change: `node docs/figures/draw-cabinet/build.mjs --all` (rebuilds and rewrites the hashes).
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const WEB = path.resolve(__dirname, '..', '..', '..');
const read = (...p: string[]) => readFileSync(path.join(WEB, ...p), 'utf8');
const FIGURES = ['draw-cabinet-tour.html', 'draw-cabinet-tour-light.html'];
const sha = (text: string) => `'sha256-${createHash('sha256').update(text, 'utf8').digest('base64')}'`;

interface Rule {
  source: string;
  headers: { key: string; value: string }[];
}
const rules = (JSON.parse(read('vercel.json')) as { headers: Rule[] }).headers;
const rule = (source: string) => {
  const r = rules.find((x) => x.source === source);
  if (!r) throw new Error(`vercel.json has no ${source} rule`);
  return Object.fromEntries(r.headers.map((h) => [h.key, h.value]));
};
const directive = (csp: string, name: string) =>
  (
    csp
      .split(';')
      .map((d) => d.trim())
      .find((d) => d.startsWith(`${name} `)) ?? ''
  )
    .split(' ')
    .slice(1);

/** The text of the page's inline <script> and <style> elements (the styles sit before the one module script). */
function inline(html: string) {
  const at = html.indexOf('<script type="module">');
  const all = (tag: string, text: string) =>
    [...text.matchAll(new RegExp(`<${tag}[^>]*>([\\s\\S]*?)</${tag}>`, 'g'))].map((m) => m[1] ?? '');
  return { scripts: all('script', html.slice(at)).slice(0, 1), styles: all('style', html.slice(0, at)) };
}

describe('the embedded draw cabinet', () => {
  const model = read('src', 'screens', 'how', 'cabinet-model.json').replace(/\r\n/g, '\n');
  const modelSha = createHash('sha256').update(model, 'utf8').digest('hex');

  it.each(FIGURES)('%s was built from the current model, with no network and one module script', (f) => {
    const html = read('public', 'figures', f);
    expect(html).toContain(`"modelSha256":"${modelSha}"`);
    expect(html.match(/<\/script>/g)?.length).toBe(1);
    expect(html).not.toMatch(/\bfetch\(|XMLHttpRequest|WebSocket|<link\b|<img\b|src="http/);
    expect(html).toContain('parent.postMessage({figure:"draw-cabinet"');
  });

  it.each(FIGURES)('%s runs under the figure CSP: its script and styles by hash', (f) => {
    const csp = rule('/figures/(.*)')['Content-Security-Policy'] ?? '';
    const { scripts, styles } = inline(read('public', 'figures', f));
    expect(scripts).toHaveLength(1);
    expect(styles.length).toBeGreaterThanOrEqual(1);
    for (const s of scripts) expect(directive(csp, 'script-src')).toContain(sha(s));
    for (const s of styles) expect(directive(csp, 'style-src-elem')).toContain(sha(s));
  });

  it('the figure rule frames the figures from this site only; the site still refuses every frame', () => {
    const fig = rule('/figures/(.*)');
    const site = rule('/((?!figures/).*)');
    const figCsp = fig['Content-Security-Policy'] ?? '';
    const siteCsp = site['Content-Security-Policy'] ?? '';
    expect(directive(figCsp, 'default-src')).toEqual(["'none'"]);
    expect(directive(figCsp, 'frame-ancestors')).toEqual(["'self'"]);
    expect(directive(figCsp, 'script-src').every((s) => s.startsWith("'sha256-"))).toBe(true);
    expect(directive(figCsp, 'connect-src')).toEqual([]);
    expect(fig['X-Frame-Options']).toBe('SAMEORIGIN');
    // F6 security review: opened directly, the figure runs in an opaque origin (scripts only), never the site's.
    expect(directive(figCsp, 'sandbox')).toEqual(['allow-scripts']);
    // Both rules deny the powerful browser features (camera, microphone, location, ...), with one value.
    for (const f of ['camera', 'microphone', 'geolocation', 'payment', 'usb']) {
      expect(site['Permissions-Policy'] ?? '').toContain(`${f}=()`);
    }
    expect(fig['Permissions-Policy']).toBe(site['Permissions-Policy']);
    expect(directive(siteCsp, 'frame-ancestors')).toEqual(["'none'"]);
    expect(directive(siteCsp, 'frame-src')).toEqual(["'self'"]);
    expect(site['X-Frame-Options']).toBe('DENY');
  });

  it('the two rules never apply to the same path', () => {
    const match = (source: string, p: string) => new RegExp(`^${source}$`).test(p);
    expect(rules.map((r) => r.source).sort()).toEqual(['/((?!figures/).*)', '/figures/(.*)']);
    for (const p of ['/', '/index.html', '/assets/index-abc.js', '/figures-x', '/figures/draw-cabinet-tour.html']) {
      const n = rules.filter((r) => match(r.source, p)).length;
      expect(n, p).toBe(1);
    }
    expect(match('/figures/(.*)', '/figures/draw-cabinet-tour.html')).toBe(true);
  });

  it("the figure's colours are the page's tokens in both themes", () => {
    const tokens = read('src', 'theme', 'tokens.css');
    const embed = readFileSync(path.join(WEB, '..', 'docs', 'figures', 'draw-cabinet', 'embed.css'), 'utf8');
    const block = (css: string, selector: RegExp) => {
      const m = selector.exec(css);
      if (!m) throw new Error(`no block ${selector}`);
      return css.slice(m.index, css.indexOf('}', m.index));
    };
    const vars = (text: string, prefix: string) =>
      Object.fromEntries(
        [...text.matchAll(new RegExp(`--${prefix}([a-z-]+):\\s*(#[0-9a-fA-F]{6})`, 'g'))].map((m) => [
          m[1],
          m[2]?.toLowerCase(),
        ]),
      );
    const themes: [RegExp, RegExp][] = [
      [/\[data-theme='dark'\] \{/, /^\.iso\{/m],
      [/\[data-theme='light'\] \{/, /^\.iso\[data-theme="light"\]\{/m],
    ];
    for (const [tokenSel, embedSel] of themes) {
      const t = vars(block(tokens, tokenSel), '');
      const e = vars(block(embed, embedSel), 'kl-');
      expect(Object.keys(e).length).toBeGreaterThanOrEqual(9);
      for (const [name, value] of Object.entries(e)) expect(value, name).toBe(t[name]);
    }
  });
});
