import { describe, expect, it } from 'vitest';
import { href, routeOf } from './routes';

describe('routes', () => {
  it('maps hashes to routes, Play by default', () => {
    expect(routeOf('', false)).toBe('play');
    expect(routeOf('#/tickets', false)).toBe('tickets');
    expect(routeOf('#/rounds?x=1', false)).toBe('rounds');
    expect(routeOf('#/nope', false)).toBe('play');
    expect(routeOf('#/catalogue', true)).toBe('play');
  });
  it('has the catalogue in development only', () => {
    expect(routeOf('#/dev/catalogue', true)).toBe('catalogue');
    expect(routeOf('#/dev/catalogue', false)).toBe('play');
    expect(href('catalogue')).toBe('#/dev/catalogue');
  });
});
