// Small shared pieces: avatars, pills, menus, modal frame, toasts, tooltip.
import { h, cx } from './dom.js';
import { icon } from './icons.js';
import { state, closeMenu, closeModal, render } from './store.js';
import { t, tn, fill } from './i18n.js';
import * as U from './util.js';

/**
 * Translated text in which words between two pairs of asterisks come out
 * bold, as an array of strings and <b> elements.
 * The marks are read from the app's own text before any value is filled in,
 * so a name or title that contains asterisks stays as it is.
 */
export function rich(text, vars) {
  return t(text).split(/\*\*(.+?)\*\*/g).map((part, i) => (i % 2 ? h('b', null, fill(part, vars)) : fill(part, vars)));
}

const AVATAR_TINTS = ['#dbeafe', '#e0e7ff', '#fce7f3', '#dcfce7', '#fef3c7', '#ede9fe', '#cffafe', '#ffe4e6'];
const failedImages = new Set();

function tintFor(text) {
  let hash = 0;
  for (const ch of text) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return AVATAR_TINTS[hash % AVATAR_TINTS.length];
}

export function avatar(user, size = 28) {
  if (!user) return h('span', { class: `avatar s${size} none`, 'data-tip': t('Unassigned') }, icon('user', Math.round(size * 0.55)));
  const src = U.avatarUrl(user.avatarUrl, size * 2);
  const img = src && !failedImages.has(src)
    ? h('img', { src, alt: '', loading: 'lazy', style: { position: 'absolute', inset: '0' }, onerror: (e) => { failedImages.add(src); e.target.remove(); } })
    : null;
  return h('span', { class: `avatar s${size}`, 'data-tip': U.displayName(user), style: { background: tintFor(user.login || '?') } }, U.initials(user), img);
}

export function avatars(users, size = 24, max = 3) {
  if (!users || users.length === 0) return avatar(null, size);
  const shown = users.slice(0, max);
  return h('span', { class: 'avatars' }, shown.map((u) => avatar(u, size)), users.length > max ? h('span', { class: `avatar s${size}` }, '+' + (users.length - max)) : null);
}

function statusGlyph(status, size = 14) {
  const base = { width: size, height: size, viewBox: '0 0 16 16', 'aria-hidden': 'true' };
  const ring = h('circle', { cx: 8, cy: 8, r: 6.2, fill: 'none', stroke: 'currentColor', 'stroke-width': 1.6 });
  switch (status) {
    case 'in_progress':
      return h('svg', base, ring, h('path', { d: 'M8 3.6a4.4 4.4 0 0 1 0 8.8z', fill: 'currentColor' }));
    case 'in_review':
      return h('svg', base, ring, h('circle', { cx: 8, cy: 8, r: 2.6, fill: 'currentColor' }));
    case 'done':
      return h('svg', base, h('circle', { cx: 8, cy: 8, r: 7, fill: 'currentColor' }), h('path', { d: 'M4.8 8.2l2.2 2.2 4.2-4.6', fill: 'none', stroke: '#fff', 'stroke-width': 1.7, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }));
    case 'canceled':
      return h('svg', base, ring, h('path', { d: 'M5.6 5.6l4.8 4.8M10.4 5.6l-4.8 4.8', fill: 'none', stroke: 'currentColor', 'stroke-width': 1.6, 'stroke-linecap': 'round' }));
    default:
      return h('svg', base, h('circle', { cx: 8, cy: 8, r: 6.2, fill: 'none', stroke: 'currentColor', 'stroke-width': 1.6, 'stroke-dasharray': '2.6 2.2' }));
  }
}

export function statusPill(status, onClick) {
  return h(onClick ? 'button' : 'span', { class: `pill ${status}`, type: onClick ? 'button' : null, onclick: onClick || null }, statusGlyph(status), U.statusLabel(status), onClick ? icon('chevDown', 12) : null);
}
export const statusIcon = (status, size = 16) => h('span', { class: `pill ${status}`, style: { border: '0', padding: '0', height: 'auto', background: 'none' }, 'data-tip': U.statusLabel(status) }, statusGlyph(status, size));

export function priorityPill(priority, onClick) {
  if (!priority) {
    if (!onClick) return h('span', { class: 'muted' }, '–');
    return h('button', { class: 'pill plain', type: 'button', onclick: onClick }, icon('flag', 13), t('No priority'), icon('chevDown', 12));
  }
  return h(onClick ? 'button' : 'span', { class: `pill ${priority}`, type: onClick ? 'button' : null, onclick: onClick || null }, icon('flag', 13), U.priorityLabel(priority), onClick ? icon('chevDown', 12) : null);
}

export function dueLabel(task) {
  const info = U.dueInfo(task.due, U.isOpen(task));
  if (!info) return h('span', { class: 'muted' }, '–');
  return h('span', { class: cx('due', info.tone), 'data-tip': info.tone === 'over' ? tn(-info.days, 'Overdue by {n} day', 'Overdue by {n} days') : null }, info.text);
}

export function pullChip(p, onClick) {
  const kind = U.pullKind(p);
  return h(onClick ? 'button' : 'span', { class: `pr-chip ${kind}`, type: onClick ? 'button' : null, 'data-tip': t('{state} pull request: {title}', { state: U.pullLabel(kind), title: p.title }), onclick: onClick || null }, icon(kind === 'merged' ? 'merge' : 'pr', 13), '#' + p.number);
}

export function pullPill(p) {
  const kind = U.pullKind(p);
  return h('span', { class: `pill ${kind}` }, icon(kind === 'merged' ? 'merge' : kind === 'closed' ? 'x' : 'pr', 13), U.pullLabel(kind));
}

export function labelTag(l) {
  return h('span', { class: 'tag' }, h('i', { style: { background: '#' + (l.color || 'cccccc') } }), l.name);
}

export function spinner() {
  return h('span', { class: 'spinner', role: 'status', 'aria-label': t('Loading') });
}

export function empty(iconName, title, text, action) {
  return h('div', { class: 'empty' }, icon(iconName, 30), h('b', null, title), text ? h('span', null, text) : null, action || null);
}

// ---- menu ------------------------------------------------------------------

export function menuView() {
  const m = state.menu;
  const q = (m.search || '').trim().toLowerCase();
  const items = m.items.filter((it) => it === 'sep' || it.title || !q || it.label.toLowerCase().includes(q) || (it.sub || '').toLowerCase().includes(q));
  const style = { top: m.y + 'px', 'min-width': (m.minWidth || 190) + 'px' };
  if (m.alignRight) style.right = Math.max(8, window.innerWidth - m.x) + 'px';
  else style.left = m.x + 'px';
  const real = items.filter((it) => it !== 'sep' && !it.title);
  return [
    h('div', { class: 'scrim clear', key: 'menu-scrim', onclick: closeMenu, oncontextmenu: (e) => { e.preventDefault(); closeMenu(); } }),
    h('div', { class: 'menu', role: 'menu', key: 'menu', style },
      m.search != null
        ? h('label', { class: 'search' }, icon('search', 15), h('input', { placeholder: m.placeholder || t('Search…'), value: m.search, 'data-autofocus': '', oninput: (e) => { m.search = e.target.value; render(); } }))
        : null,
      items.map((it) => {
        if (it === 'sep') return h('div', { class: 'menu-sep' });
        if (it.title) return h('div', { class: 'menu-title' }, it.title);
        return h('button', { class: cx('menu-item', it.danger && 'danger'), type: 'button', role: 'menuitem', disabled: !!it.disabled, onclick: () => { closeMenu(); if (it.onClick) it.onClick(); } },
          it.node || (it.icon ? icon(it.icon, 16) : null),
          h('span', { class: 'grow' }, it.label, it.sub ? h('small', null, it.sub) : null),
          it.checked ? icon('check', 16, { class: 'check' }) : null);
      }),
      real.length === 0 ? h('div', { class: 'menu-title' }, t('Nothing found')) : null),
  ];
}

/** Keeps the open menu inside the window (called after each draw). */
export function fitMenu() {
  const el = document.querySelector('#layer .menu');
  if (!el || !state.menu) return;
  const r = el.getBoundingClientRect();
  if (r.bottom > window.innerHeight - 8) {
    state.menu.y = Math.max(8, window.innerHeight - r.height - 8);
    el.style.top = state.menu.y + 'px';
  }
  if (!state.menu.alignRight && r.right > window.innerWidth - 8) {
    state.menu.x = Math.max(8, window.innerWidth - r.width - 8);
    el.style.left = state.menu.x + 'px';
  }
  const input = el.querySelector('[data-autofocus]');
  if (input && !state.menu.focused) {
    state.menu.focused = true;
    input.focus();
  }
}

// ---- modal frame -----------------------------------------------------------------

export function modalFrame({ title, body, foot, wide, onSubmit }) {
  return [
    h('div', { class: 'scrim', key: 'modal-scrim', onclick: closeModal }),
    h('div', { class: 'modal-wrap', key: 'modal', onclick: (e) => { if (e.target === e.currentTarget) closeModal(); } },
      h('form', { class: cx('modal', wide && 'wide'), role: 'dialog', 'aria-modal': 'true', 'aria-label': title, novalidate: true, onsubmit: (e) => { e.preventDefault(); if (onSubmit) onSubmit(); } },
        h('div', { class: 'modal-head' }, h('h2', null, title), h('button', { class: 'icon-btn ghost', type: 'button', 'aria-label': t('Close'), onclick: closeModal }, icon('x', 18))),
        h('div', { class: 'modal-body' }, body),
        h('div', { class: 'modal-foot' }, foot))),
  ];
}

export function toastsView() {
  return h('div', { class: 'toasts', key: 'toasts', 'aria-live': 'polite' }, state.toasts.map((x) => h('div', { class: cx('toast', x.kind === 'err' && 'err'), key: 't' + x.id }, icon(x.kind === 'err' ? 'alert' : 'checkCircle', 16), h('span', null, x.text))));
}

// ---- tooltip: any element with data-tip ---------------------------------------------

export function installTooltips() {
  const tip = document.getElementById('tip');
  let current = null;
  let shown = '';
  const place = (x, y) => {
    const r = tip.getBoundingClientRect();
    let left = x + 12;
    let top = y + 16;
    if (left + r.width > window.innerWidth - 8) left = Math.max(8, x - r.width - 12);
    if (top + r.height > window.innerHeight - 8) top = Math.max(8, y - r.height - 12);
    tip.style.left = left + 'px';
    tip.style.top = top + 'px';
  };
  const show = (el, x, y) => {
    current = el;
    shown = el.getAttribute('data-tip');
    // textContent: tooltip text often comes from GitHub (titles, names).
    tip.textContent = shown;
    tip.hidden = false;
    place(x, y);
  };
  const hide = () => {
    current = null;
    shown = '';
    tip.hidden = true;
  };
  document.addEventListener('pointermove', (e) => {
    const el = e.target instanceof Element ? e.target.closest('[data-tip]') : null;
    const text = el ? el.getAttribute('data-tip') : '';
    if (!text) return void (current && hide());
    // The canvas changes its own tip as the pointer moves over different commits.
    if (el !== current || text !== shown) show(el, e.clientX, e.clientY);
    else place(e.clientX, e.clientY);
  });
  document.addEventListener('pointerdown', hide);
  document.addEventListener('scroll', hide, true);
  document.addEventListener('focusin', (e) => {
    const el = e.target instanceof Element ? e.target.closest('[data-tip]') : null;
    if (el && el.getAttribute('data-tip') && el.matches(':focus-visible')) {
      const r = el.getBoundingClientRect();
      show(el, r.left + r.width / 2, r.top);
    }
  });
  document.addEventListener('focusout', hide);
}
