// One door to the Rust core. Three ways through it, picked automatically:
//   1. inside the desktop app: Tauri's invoke;
//   2. in a normal browser next to the dev server: POST /api/<method>;
//   3. with ?mock in the address: an in-page fake with demo data.

import { t } from './i18n.js';

const query = new URLSearchParams(location.search);
export const MOCK = query.has('mock');
export const SHOWCASE = query.has('showcase');

const tauri = window.__TAURI__ && window.__TAURI__.core;
export const IS_APP = !!tauri;

let mock = null;

/** Calls a core method. Rejects with { code, message }. */
export async function api(method, params = {}) {
  if (MOCK) {
    mock = mock || (await import('./mock.js'));
    return mock.call(method, params);
  }
  if (tauri) return tauri.invoke('api', { method, params });
  let res;
  try {
    res = await fetch('/api/' + method, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Headway': '1' },
      body: JSON.stringify(params),
    });
  } catch {
    throw { code: 'network', message: 'The Headway dev server is not answering.' };
  }
  const json = await res.json().catch(() => null);
  if (!json) throw { code: 'error', message: `Unexpected answer (${res.status}).` };
  if (!json.ok) throw json.error;
  return json.data;
}

/** Opens a link in the system browser (never inside the app window). */
export function openUrl(url) {
  if (!url) return;
  if (MOCK) return void window.open(url, '_blank', 'noopener');
  api('open_url', { url }).catch(() => window.open(url, '_blank', 'noopener'));
}

/** Native folder picker; resolves to null where there is none (browser preview). */
export async function pickFolder() {
  const dialog = window.__TAURI__ && window.__TAURI__.dialog;
  if (!dialog) return null;
  const picked = await dialog.open({ directory: true, multiple: false, title: t('Choose the folder of your local clone') });
  return typeof picked === 'string' ? picked : null;
}
export const HAS_FOLDER_PICKER = !!(window.__TAURI__ && window.__TAURI__.dialog);

export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
