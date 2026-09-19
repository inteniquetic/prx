// Colour maths shared by the token check and the rendered-page check.
//
// Tokens are authored in oklch, which says nothing about how much contrast two
// of them have; WCAG is defined on sRGB relative luminance. So everything is
// converted down to sRGB before anything is judged.

/** oklch(L C H) with L in 0..1 (or a percentage) -> [r, g, b] in 0..255. */
export function oklchToRgb(l, c, hDeg) {
  const h = (hDeg * Math.PI) / 180;
  return oklabToRgb(l, c * Math.cos(h), c * Math.sin(h));
}

/** oklab(L a b) -> [r, g, b] in 0..255. Chromium reports computed colours this way. */
export function oklabToRgb(l, a, b) {
  const l_ = l + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = l - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = l - 0.0894841775 * a - 1.291485548 * b;

  const L = l_ ** 3;
  const M = m_ ** 3;
  const S = s_ ** 3;

  const lin = [
    4.0767416621 * L - 3.3077115913 * M + 0.2309699292 * S,
    -1.2684380046 * L + 2.6097574011 * M - 0.3413193965 * S,
    -0.0041960863 * L - 0.7034186147 * M + 1.707614701 * S
  ];

  return lin.map((v) => {
    const clamped = Math.min(Math.max(v, 0), 1);
    const srgb = clamped <= 0.0031308 ? 12.92 * clamped : 1.055 * clamped ** (1 / 2.4) - 0.055;
    return Math.round(srgb * 255);
  });
}

/** Parses the colour syntaxes the stylesheet actually uses. */
export function parseColor(value) {
  const text = String(value).trim();

  const oklab = text.match(/^oklab\(\s*([\d.]+%?)\s+(-?[\d.]+)\s+(-?[\d.]+)/);
  if (oklab) {
    const l = oklab[1].endsWith('%') ? parseFloat(oklab[1]) / 100 : parseFloat(oklab[1]);
    return oklabToRgb(l, parseFloat(oklab[2]), parseFloat(oklab[3]));
  }

  const oklch = text.match(/^oklch\(\s*([\d.]+%?)\s+([\d.]+)\s+([\d.]+)/);
  if (oklch) {
    const l = oklch[1].endsWith('%') ? parseFloat(oklch[1]) / 100 : parseFloat(oklch[1]);
    return oklchToRgb(l, parseFloat(oklch[2]), parseFloat(oklch[3]));
  }

  const hex = text.match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/i);
  if (hex) {
    const h = hex[1].length === 3 ? [...hex[1]].map((ch) => ch + ch).join('') : hex[1];
    return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
  }

  const rgb = text.match(/^rgba?\(([^)]+)\)$/);
  if (rgb) {
    const parts = rgb[1].split(/[\s,/]+/).filter(Boolean).map(parseFloat);
    return [parts[0], parts[1], parts[2]];
  }

  throw new Error(`unsupported colour: ${text}`);
}

/** The alpha of a colour in any of the syntaxes above; 1 when it has none. */
export function alphaOf(value) {
  const text = String(value).trim();
  const slash = text.match(/\/\s*([\d.]+%?)\s*\)$/);
  if (slash) {
    return slash[1].endsWith('%') ? parseFloat(slash[1]) / 100 : parseFloat(slash[1]);
  }
  const rgba = text.match(/^rgba\(([^)]+)\)$/);
  if (rgba) {
    const parts = rgba[1].split(/[\s,]+/).filter(Boolean);
    return parts.length > 3 ? parseFloat(parts[3]) : 1;
  }
  return 1;
}

/** `alpha` of `fg` laid over opaque `bg` — what a /10 tint actually renders as. */
export function over(fg, bg, alpha) {
  return fg.map((v, i) => v * alpha + bg[i] * (1 - alpha));
}

function channelLuminance(v) {
  const s = v / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance([r, g, b]) {
  return (
    0.2126 * channelLuminance(r) + 0.7152 * channelLuminance(g) + 0.0722 * channelLuminance(b)
  );
}

export function contrast(a, b) {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}
