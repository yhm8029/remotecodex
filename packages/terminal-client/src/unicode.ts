// Adapted from xterm.js (MIT) – scalar-combining logic inspired by
// src/common/input/UnicodeV6.ts + src/common/services/UnicodeService.ts.
// Width tables generated from Rust unicode-width 0.2.2 (MIT/Apache-2.0).
import { widthRanges } from './unicode-width.generated.js';

export function scalarWidth(codepoint: number): 0 | 1 | 2 {
  if (!Number.isInteger(codepoint) || codepoint < 0 || codepoint > 0x10FFFF) return 0;
  if (codepoint >= 0xD800 && codepoint <= 0xDFFF) return 0;
  let lo = 0;
  let hi = widthRanges.length - 1;
  while (lo <= hi) {
    const middle = (lo + hi) >>> 1;
    const range = widthRanges[middle];
    if (codepoint < range[0]) hi = middle - 1;
    else if (codepoint > range[1]) lo = middle + 1;
    else return range[2] as 0 | 1 | 2;
  }
  return 1;
}

export const unicodeProfile = {
  version: 'remotecodex-unicode17-two-cell-v1',
  wcwidth: scalarWidth,
  charProperties(codepoint: number, preceding: number): number {
    let width: number = scalarWidth(codepoint);
    let shouldJoin = width === 0 && preceding !== 0;
    if (shouldJoin) {
      const oldWidth = (preceding >> 1) & 3;
      if (oldWidth === 0) shouldJoin = false;
      else if (oldWidth > width) width = oldWidth;
    }
    return ((width & 3) << 1) | (shouldJoin ? 1 : 0);
  }
};

export function configureUnicode(terminal: {
  unicode: {
    register(provider: typeof unicodeProfile): void;
    activeVersion: string;
  };
}): void {
  terminal.unicode.register(unicodeProfile);
  terminal.unicode.activeVersion = unicodeProfile.version;
}