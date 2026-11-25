// Shared by the setup window (setup.html, inside Headway-Setup.exe) and the
// uninstall window (uninstall.html, inside headway.exe --uninstall): the
// window's own title bar, the small controls, and one way to reach the Rust
// side that also works in a plain browser, where it is only a preview.
import { h, cx } from './dom.js';
import { icon } from './icons.js';
import { t, LANGUAGES, language } from './i18n.js';

const tauri = window.__TAURI__;
export const query = new URLSearchParams(location.search);
/** Opened in a browser: a preview with made-up answers; nothing is installed or removed. */
export const PREVIEW = !tauri;

/** Calls a command of the Rust side; `fake` answers instead in the preview. */
export function invoke(command, args, fake) {
  return tauri ? tauri.core.invoke(command, args) : fake(args || {});
}

/** Hears the events the Rust side sends while it works. */
export function listen(event, fn) {
  if (tauri) tauri.event.listen(event, (e) => fn(e.payload));
}

const thisWindow = () => (tauri ? tauri.window.getCurrentWindow() : null);

export function closeWindow() {
  const w = thisWindow();
  if (w) w.close();
  else window.close();
}

/** The name on the taskbar, in the chosen language. */
export function setTitle(title) {
  document.title = title;
  const w = thisWindow();
  if (w) w.setTitle(title).catch(() => {});
}

/** Native folder picker; null in the preview or when the user cancels. */
export async function pickFolder(title, startAt) {
  if (!tauri || !tauri.dialog) return null;
  const picked = await tauri.dialog.open({ directory: true, multiple: false, title, defaultPath: startAt || undefined });
  return typeof picked === 'string' ? picked : null;
}
export const CAN_PICK_FOLDER = !!(tauri && tauri.dialog);

export const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** The strip the window is dragged by, with minimize and close on it. */
export function windowBar(busy) {
  return [
    h('div', { class: 'drag', 'data-tauri-drag-region': '' }),
    h('div', { class: 'controls' },
      h('button', { class: 'ctl', type: 'button', 'aria-label': t('Minimize'), onclick: () => { const w = thisWindow(); if (w) w.minimize(); } }, icon('minus', 16)),
      h('button', { class: 'ctl close', type: 'button', 'aria-label': t('Close'), disabled: !!busy, onclick: closeWindow }, icon('x', 16))),
  ];
}

export function brand() {
  return h('div', { class: 'brand' }, h('img', { src: 'assets/logo.svg', alt: '' }), 'Headway');
}

/** A row with an on/off switch; the whole row is the click target. */
export function switchRow(iconName, title, hint, on, onChange) {
  const flip = () => onChange(!on);
  return h('div', { class: 'row', onclick: flip },
    h('span', { class: 'ic' }, icon(iconName, 16)),
    h('span', { class: 'txt' }, h('b', null, title), hint ? h('small', null, hint) : null),
    h('button', { class: 'switch', type: 'button', role: 'switch', 'aria-checked': String(!!on), 'aria-label': title, onclick: (e) => { e.stopPropagation(); flip(); } }));
}

/** English | Español */
export function languageChoice(onPick) {
  return h('div', { class: 'seg', role: 'group', 'aria-label': t('Language') },
    LANGUAGES.map(([code, name]) => h('button', { class: cx(language() === code && 'on'), type: 'button', key: code, lang: code, 'aria-pressed': String(language() === code), onclick: () => onPick(code) }, name)));
}

/** Common behaviour of both windows. `primary` runs on Enter, `escape` on Esc. */
export function installKeys(primary, escape) {
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !(e.target instanceof HTMLButtonElement)) primary();
    else if (e.key === 'Escape') escape();
  });
  // These are small fixed windows, not web pages: no page menu, no zooming.
  window.addEventListener('contextmenu', (e) => e.preventDefault());
  window.addEventListener('wheel', (e) => { if (e.ctrlKey) e.preventDefault(); }, { passive: false });
  if (query.has('still')) document.body.classList.add('still');
}
