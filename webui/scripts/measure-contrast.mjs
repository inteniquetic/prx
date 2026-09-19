// The in-page pass, shared by the styleguide check and the shell check.
//
// It runs inside the browser, so it may only use what is on the page: the
// colour helpers injected from contrast.mjs and its own argument. Both callers
// hand it a list of root selectors, which is how the shell check can judge the
// chrome this task owns without auditing pages that later tasks rewrite.
export const MEASURE = (roots) => {
  const results = [];

  const hidden = (el) => {
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || style.display === 'none') return true;
    if (parseFloat(style.opacity) < 0.95) return true;
    if (el.closest('[aria-hidden="true"]')) return true;
    // sr-only: present for assistive tech, clipped to nothing on screen.
    const rect = el.getBoundingClientRect();
    return rect.width <= 1 || rect.height <= 1;
  };

  /** The colour behind `el`, compositing every translucent layer above it. */
  const backgroundOf = (el) => {
    const layers = [];
    for (let node = el; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      if (style.backgroundImage !== 'none') return null; // a gradient is not one colour
      const bg = style.backgroundColor;
      const alpha = alphaOf(bg);
      if (alpha === 0) continue;
      layers.unshift([bg, alpha]);
      if (alpha === 1) {
        let acc = parseColor(bg);
        for (const [color, a] of layers.slice(1)) acc = over(parseColor(color), acc, a);
        return acc;
      }
    }
    return null;
  };

  const describe = (el) => {
    const slot = el.closest('[data-slot]')?.dataset.slot;
    const text = (el.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40);
    return `${slot ? `[${slot}] ` : ''}${el.tagName.toLowerCase()} "${text}"`;
  };

  const scope = roots?.length
    ? roots.flatMap((selector) => [...document.querySelectorAll(`${selector}, ${selector} *`)])
    : [...document.querySelectorAll('body *')];

  for (const el of new Set(scope)) {
    const ownText = [...el.childNodes].some(
      (n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim().length > 0
    );
    if (!ownText || hidden(el)) continue;

    const style = getComputedStyle(el);
    const background = backgroundOf(el);
    if (!background) continue;

    const size = parseFloat(style.fontSize);
    const weight = Number(style.fontWeight) || 400;
    const large = size >= 24 || (size >= 18.66 && weight >= 700);
    const min = large ? 3 : 4.5;

    const fgAlpha = alphaOf(style.color);
    const foreground =
      fgAlpha === 1 ? parseColor(style.color) : over(parseColor(style.color), background, fgAlpha);

    results.push({
      what: describe(el),
      ratio: contrast(foreground, background),
      min,
      size,
      fg: style.color,
      bg: style.backgroundColor
    });
  }

  return results;
};
