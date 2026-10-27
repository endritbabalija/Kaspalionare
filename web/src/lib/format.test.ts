import { describe, expect, it } from 'vitest';
import { clock, countdown, daaToMs, groupDigits, kas2, kasExact, shortHash, sompiExact, sompiOfKas } from './format';

describe('amounts', () => {
  it('prints KAS with two decimals, rounded half up', () => {
    expect(kas2('4820000000000')).toBe('48,200.00');
    expect(kas2('0')).toBe('0.00');
    expect(kas2('32254200')).toBe('0.32');
    expect(kas2('32500000')).toBe('0.33');
    expect(kas2('32499999')).toBe('0.32');
    expect(kas2('99999999')).toBe('1.00');
    expect(kas2('-150000000')).toBe('-1.50');
    expect(kas2('9223372036854775807')).toBe('92,233,720,368.55');
  });
  it('prints exact sompi grouped', () => {
    expect(sompiExact('4820000000000')).toBe('4,820,000,000,000');
    expect(sompiExact('13000000')).toBe('13,000,000');
    expect(groupDigits('285012')).toBe('285,012');
  });
  it('reads an amount typed in KAS as sompi (F6), refusing anything else', () => {
    expect(sompiOfKas('10')).toBe('1000000000');
    expect(sompiOfKas(' 12.5 ')).toBe('1250000000');
    expect(sompiOfKas('0.00000001')).toBe('1');
    expect(sompiOfKas('99999999999.99999999')).toBe('9999999999999999999');
    for (const bad of ['', '.5', '1.', '1.123456789', '-1', '1e9', '1,5', '0x10', '123456789012', 'NaN']) {
      expect(sompiOfKas(bad), bad).toBeNull();
    }
  });
  it('prints KAS exactly in the form the amount field reads back (F6 part A review S1)', () => {
    expect(kasExact('3000000')).toBe('0.03');
    expect(kasExact('2510000')).toBe('0.0251');
    expect(kasExact('1')).toBe('0.00000001');
    expect(kasExact('0')).toBe('0.00');
    expect(kasExact('123456789000000')).toBe('1234567.89');
    for (const v of ['0', '1', '999999', '1000000', '2510000', '100000000', '1499999999', '9999999999999999999']) {
      expect(sompiOfKas(kasExact(v)), v).toBe(v);
    }
  });
  it('refuses what is not a decimal', () => {
    expect(() => kas2('1e9')).toThrow();
    expect(() => kas2('')).toThrow();
  });
});

describe('time', () => {
  it('converts DAA at 10 per second', () => {
    expect(daaToMs('1600', '1000', 1_000_000)).toBe(1_060_000);
    expect(daaToMs('900', '1000', 1_000_000)).toBe(990_000);
  });
  it('formats a countdown', () => {
    expect(countdown(0)).toBe('00:00:00');
    expect(countdown(-5000)).toBe('00:00:00');
    expect(countdown((5 * 3600 + 42 * 60 + 10) * 1000 + 999)).toBe('05:42:10');
    expect(countdown((2 * 86400 + 3661) * 1000)).toBe('2d 01:01:01');
  });
  it('shows a clock, with the date when not today', () => {
    const now = new Date(2026, 9, 25, 9, 0).getTime();
    expect(clock(new Date(2026, 9, 25, 14, 32).getTime(), now)).toBe('14:32');
    expect(clock(new Date(2026, 10, 2, 8, 5).getTime(), now)).toBe('2 Nov 08:05');
  });
});

describe('hashes', () => {
  it('shortens hashes and addresses', () => {
    expect(shortHash('9f3c0a1122334455667788b71e21')).toBe('9f3c0a…b71e21');
    expect(shortHash('kaspatest:qqdasdj3abcdefghijklmnopphyfvu')).toBe('kaspatest:qqdasd…phyfvu');
    expect(shortHash('abc')).toBe('abc');
  });
});
