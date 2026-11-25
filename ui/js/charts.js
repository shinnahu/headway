// The four chart forms on the Project health screen, drawn as plain SVG.
// Every mark has a hover/focus tooltip and each chart carries a hidden table
// with the same numbers, so nothing is readable by colour or hover alone.
import { h } from './dom.js';
import { clamp } from './util.js';

/** Progress ring for a 0-100 value. */
export function ring(value, size = 42) {
  const v = clamp(Math.round(value), 0, 100);
  const stroke = 4.5;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const mid = size / 2;
  return h('svg', { class: 'ring', width: size, height: size, viewBox: `0 0 ${size} ${size}`, role: 'img', 'aria-label': `${v} percent` },
    h('circle', { class: 'track', cx: mid, cy: mid, r, fill: 'none', 'stroke-width': stroke }),
    v > 0
      ? h('circle', { cx: mid, cy: mid, r, fill: 'none', stroke: 'currentColor', 'stroke-width': stroke, 'stroke-linecap': 'round', 'stroke-dasharray': `${((c * v) / 100).toFixed(2)} ${c.toFixed(2)}`, transform: `rotate(-90 ${mid} ${mid})` })
      : null);
}

/** Hatched progress bar. */
export function meter(value, label) {
  const v = clamp(value, 0, 100);
  return h('div', { class: 'meter', role: 'img', 'aria-label': label || `${Math.round(v)} percent` }, h('i', { style: { width: v + '%' } }));
}

function niceMax(v) {
  if (v <= 4) return 4;
  const pow = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 1.5, 2, 3, 4, 5, 6, 8, 10]) if (m * pow >= v) return m * pow;
  return 10 * pow;
}

function hiddenTable(caption, head, rows) {
  return h('table', { class: 'sr-only' },
    h('caption', null, caption),
    h('thead', null, h('tr', null, head.map((c) => h('th', { scope: 'col' }, c)))),
    h('tbody', null, rows.map((r) => h('tr', null, r.map((c) => h('td', null, String(c)))))));
}

/**
 * Columns over time. items: [{ label, value, light, tip, name }]
 * `light` marks the earlier period: the same blue, stepped back.
 */
export function barChart(items, { height = 190, caption = 'Chart', unit = '' } = {}) {
  const W = 640;
  const padL = 26, padR = 6, padT = 8, padB = 24;
  const plotH = height - padT - padB;
  const max = niceMax(Math.max(0, ...items.map((d) => d.value)));
  const slot = (W - padL - padR) / Math.max(1, items.length);
  const bw = Math.min(38, slot * 0.72);
  const y = (v) => padT + plotH - (v / max) * plotH;
  const ticks = [0, max / 2, max];

  const defs = h('defs', null,
    h('pattern', { id: 'hw-hatch-strong', width: 7, height: 7, patternUnits: 'userSpaceOnUse', patternTransform: 'rotate(45)' },
      h('rect', { width: 7, height: 7, style: { fill: 'var(--accent)' } }),
      h('rect', { width: 2.6, height: 7, fill: '#ffffff', opacity: 0.2 })),
    h('pattern', { id: 'hw-hatch-light', width: 7, height: 7, patternUnits: 'userSpaceOnUse', patternTransform: 'rotate(45)' },
      h('rect', { width: 7, height: 7, style: { fill: 'var(--de-emph)' } }),
      h('rect', { width: 2.6, height: 7, fill: '#ffffff', opacity: 0.55 })));

  const grid = ticks.map((t) => h('g', null,
    h('line', { class: 'grid', x1: padL, x2: W - padR, y1: y(t), y2: y(t) }),
    h('text', { class: 'axis', x: padL - 8, y: y(t) + 4, 'text-anchor': 'end' }, String(Math.round(t)))));

  const bars = items.map((d, i) => {
    const cx = padL + slot * i + slot / 2;
    const top = d.value > 0 ? Math.min(y(d.value), y(0) - 4) : y(0) - 3;
    const hgt = y(0) - top;
    const tip = d.tip || `${d.name || d.label}\n${d.value}${unit ? ' ' + unit : ''}`;
    return h('g', null,
      h('rect', {
        class: 'bar', x: cx - bw / 2, y: top, width: bw, height: hgt, rx: Math.min(8, hgt / 2, bw / 2),
        fill: d.light ? 'url(#hw-hatch-light)' : 'url(#hw-hatch-strong)', opacity: d.value > 0 ? 1 : 0.55,
        tabindex: 0, role: 'img', 'aria-label': tip.replace('\n', ': '), 'data-tip': tip,
      }),
      // The whole column is the hover target, not just the painted bar.
      h('rect', { class: 'hit', x: cx - slot / 2, y: padT, width: slot, height: plotH, 'data-tip': tip }),
      h('text', { class: 'axis', x: cx, y: height - 6, 'text-anchor': 'middle' }, d.label));
  });

  return h('figure', { style: { margin: '0' } },
    h('svg', { viewBox: `0 0 ${W} ${height}`, role: 'img', 'aria-label': caption }, defs, grid, bars),
    hiddenTable(caption, ['Day', 'Value'], items.map((d) => [d.name || d.label, d.value])));
}

/** A single trend line with markers. points: [{ label, value, tip, name }] */
export function lineChart(points, { height = 190, caption = 'Chart' } = {}) {
  const W = 360;
  const padL = 26, padR = 12, padT = 12, padB = 24;
  const plotH = height - padT - padB;
  const values = points.map((p) => p.value);
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  // Counts start at zero unless the numbers are large enough that the change would vanish.
  if (lo < 12 || hi - lo > hi * 0.5) lo = 0;
  else lo = Math.floor(lo - (hi - lo) * 0.4);
  hi = lo === 0 ? niceMax(hi) : Math.ceil(hi + (hi - lo) * 0.15);
  if (hi === lo) hi = lo + 1;
  const step = points.length > 1 ? (W - padL - padR) / (points.length - 1) : 0;
  const x = (i) => padL + step * i;
  const y = (v) => padT + plotH - ((v - lo) / (hi - lo)) * plotH;
  const path = points.map((p, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)} ${y(p.value).toFixed(1)}`).join(' ');
  const area = `${path} L${x(points.length - 1).toFixed(1)} ${y(lo).toFixed(1)} L${x(0).toFixed(1)} ${y(lo).toFixed(1)} Z`;
  const ticks = [lo, (lo + hi) / 2, hi];

  return h('figure', { style: { margin: '0' } },
    h('svg', { viewBox: `0 0 ${W} ${height}`, role: 'img', 'aria-label': caption },
      h('defs', null, h('linearGradient', { id: 'hw-area', x1: 0, y1: 0, x2: 0, y2: 1 },
        h('stop', { offset: 0, style: { 'stop-color': 'var(--accent)' }, 'stop-opacity': 0.2 }),
        h('stop', { offset: 1, style: { 'stop-color': 'var(--accent)' }, 'stop-opacity': 0 }))),
      ticks.map((t) => h('g', null,
        h('line', { class: 'grid', x1: padL, x2: W - padR, y1: y(t), y2: y(t) }),
        h('text', { class: 'axis', x: padL - 8, y: y(t) + 4, 'text-anchor': 'end' }, String(Math.round(t))))),
      points.length > 1 ? h('path', { d: area, fill: 'url(#hw-area)' }) : null,
      points.length > 1 ? h('path', { class: 'line', d: path }) : null,
      points.map((p, i) => {
        const tip = p.tip || `${p.name || p.label}\n${p.value}`;
        const w = step || W - padL - padR;
        return h('g', null,
          h('circle', { class: 'dot', cx: x(i), cy: y(p.value), r: 4 }),
          h('rect', { class: 'hit', x: x(i) - w / 2, y: padT, width: w, height: plotH, tabindex: 0, role: 'img', 'aria-label': tip.replace('\n', ': '), 'data-tip': tip }),
          h('text', { class: 'axis', x: x(i), y: height - 6, 'text-anchor': 'middle' }, p.label));
      })),
    hiddenTable(caption, ['Day', 'Value'], points.map((p) => [p.name || p.label, p.value])));
}
