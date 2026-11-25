// A tiny rendering core: `h()` builds real DOM nodes and `morph()` patches the
// page to match a freshly built tree, keeping focus, scroll and caret where
// they are. No framework, no build step.

const SVG_NS = 'http://www.w3.org/2000/svg';
const SVG_TAGS = new Set([
  'svg', 'path', 'circle', 'rect', 'line', 'polyline', 'polygon', 'g', 'defs', 'pattern',
  'linearGradient', 'stop', 'text', 'tspan', 'clipPath', 'ellipse', 'title',
]);
const PROPS = new Set(['value', 'checked', 'selected', 'disabled', 'indeterminate']);

function dispatch(e) {
  const fn = e.currentTarget.__on && e.currentTarget.__on[e.type];
  if (fn) return fn.call(e.currentTarget, e);
}

function listen(el, type) {
  if (!el.__types) el.__types = new Set();
  if (!el.__types.has(type)) {
    el.__types.add(type);
    el.addEventListener(type, dispatch);
  }
}

function append(el, children) {
  for (const c of children) {
    if (c == null || c === false || c === true) continue;
    if (Array.isArray(c)) append(el, c);
    else if (c instanceof Node) el.appendChild(c);
    else el.appendChild(document.createTextNode(String(c)));
  }
}

/** h('div', { class: 'x', onclick: fn }, child, ...) */
export function h(tag, props, ...children) {
  const svg = SVG_TAGS.has(tag);
  const el = svg ? document.createElementNS(SVG_NS, tag) : document.createElement(tag);
  if (props) {
    for (const k in props) {
      const v = props[k];
      if (v == null || v === false) continue;
      if (k === 'class') {
        el.setAttribute('class', Array.isArray(v) ? v.filter(Boolean).join(' ') : v);
      } else if (k === 'style') {
        for (const sk in v) if (v[sk] != null) el.style.setProperty(sk, String(v[sk]));
      } else if (k[0] === 'o' && k[1] === 'n' && typeof v === 'function') {
        const type = k.slice(2);
        (el.__on || (el.__on = {}))[type] = v;
        listen(el, type);
      } else if (k === 'key') {
        el.__key = v;
      } else if (k === 'html') {
        // Only ever given sanitized HTML (see sanitize.js).
        el.__html = v;
        el.innerHTML = v;
      } else if (k === 'keep') {
        // morph() leaves this element's children alone (canvas hosts and the like).
        el.__keep = true;
      } else if (PROPS.has(k)) {
        el[k] = v;
        el.__props = el.__props || {};
        el.__props[k] = v;
      } else if (v === true) {
        el.setAttribute(k, '');
      } else {
        el.setAttribute(k, String(v));
      }
    }
  }
  append(el, children);
  return el;
}

const same = (a, b) => a.nodeType === b.nodeType && a.nodeName === b.nodeName && a.__key === b.__key;

/** Makes `from` look like `to`; returns the node that is now in the document. */
export function morph(from, to) {
  if (!same(from, to)) {
    from.replaceWith(to);
    return to;
  }
  if (from.nodeType !== 1) {
    if (from.nodeValue !== to.nodeValue) from.nodeValue = to.nodeValue;
    return from;
  }
  // attributes
  for (const a of Array.from(from.attributes)) {
    if (a.name !== 'style' && !to.hasAttribute(a.name)) from.removeAttribute(a.name);
  }
  for (const a of Array.from(to.attributes)) {
    if (a.name !== 'style' && from.getAttribute(a.name) !== a.value) from.setAttribute(a.name, a.value);
  }
  if (from.style.cssText !== to.style.cssText) from.style.cssText = to.style.cssText;
  // handlers
  from.__on = to.__on;
  if (to.__types) for (const t of to.__types) listen(from, t);
  // live properties (a field being typed in keeps its text)
  const before = from.__props || {};
  const after = to.__props || {};
  for (const k in after) {
    const typing = k === 'value' && document.activeElement === from;
    if (!typing && from[k] !== after[k]) from[k] = after[k];
  }
  for (const k in before) {
    if (!(k in after) && k !== 'value') from[k] = false;
  }
  from.__props = to.__props;

  if (to.__html !== undefined) {
    if (from.__html !== to.__html) from.innerHTML = to.__html;
    from.__html = to.__html;
    return from;
  }
  from.__html = undefined;
  if (from.__keep && to.__keep) return from;
  from.__keep = to.__keep;
  morphChildren(from, to);
  return from;
}

function morphChildren(from, to) {
  const keyed = new Map();
  for (let c = from.firstChild; c; c = c.nextSibling) {
    if (c.__key != null) keyed.set(c.__key, c);
  }
  let cursor = from.firstChild;
  for (const next of Array.from(to.childNodes)) {
    let match = null;
    if (next.__key != null) {
      match = keyed.get(next.__key) || null;
      if (match) keyed.delete(next.__key);
    } else {
      // The next unkeyed node of the same kind, if it is the one at the cursor.
      if (cursor && cursor.__key == null && same(cursor, next)) match = cursor;
    }
    if (match) {
      if (match === cursor) cursor = cursor.nextSibling;
      else from.insertBefore(match, cursor);
      morph(match, next);
    } else {
      from.insertBefore(next, cursor);
    }
  }
  while (cursor) {
    const dead = cursor;
    cursor = cursor.nextSibling;
    from.removeChild(dead);
  }
}

/** Renders `tree` into `root` (which holds exactly one child). */
export function mount(root, tree) {
  if (root.firstChild) morph(root.firstChild, tree);
  else root.appendChild(tree);
}

export const cx = (...names) => names.filter(Boolean).join(' ');
