// Two languages. English is the text written in the code; Spanish lives in
// i18n/es.js, keyed by the English text. A string without a translation falls
// back to English, and scripts/check-i18n.mjs lists any that are missing.
//
// Only the app's own words go through t(). Text that people wrote (task
// titles, comments, names) is never translated.
import { ES, ES_MESSAGES, ES_ERRORS } from './i18n/es.js';

const DICTS = { es: ES };
const MESSAGES = { es: ES_MESSAGES };
const ERROR_RULES = { es: ES_ERRORS };

let current = 'en';
let chosen = null; // 'en' | 'es' | null = follow the system

export const LANGUAGES = [['en', 'English'], ['es', 'Español']];

export const systemLanguage = () => ((navigator.language || 'en').toLowerCase().startsWith('es') ? 'es' : 'en');

/** Switches language. Anything other than a known code follows the system. */
export function setLanguage(code) {
  chosen = code === 'en' || code === 'es' ? code : null;
  current = chosen || systemLanguage();
  document.documentElement.lang = current;
}

export const language = () => current;
export const chosenLanguage = () => chosen;
/** For dates and numbers. */
export const locale = () => (current === 'es' ? 'es' : 'en-US');

/** Marks text for translation without translating yet (for tables read later with t). */
export const N = (text) => text;

/** Puts values into the {name} places of a text. */
export const fill = (text, vars) => (vars ? text.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : text);

/** t('Task #{n} created', { n: 12 }) */
export function t(text, vars) {
  const dict = DICTS[current];
  return fill((dict && dict[text]) || text, vars);
}

/** tn(3, '{n} task', '{n} tasks') -> "3 tasks" / "3 tareas" */
export function tn(count, one, many, vars) {
  return t(count === 1 ? one : many, { n: Number(count || 0).toLocaleString(locale()), ...(vars || {}) });
}

/**
 * Messages from the core arrive in English. The fixed ones are in the
 * dictionary; the ones with a name or number in them match a pattern.
 * Anything else (git's or GitHub's own words) is shown as it came.
 */
export function tError(e) {
  const msg = typeof e === 'string' ? e : (e && e.message) || '';
  if (!msg) return t('Something went wrong.');
  const dict = DICTS[current];
  if (!dict) return msg;
  if (dict[msg]) return dict[msg];
  const fixed = MESSAGES[current];
  if (fixed && fixed[msg]) return fixed[msg];
  for (const [pattern, render] of ERROR_RULES[current] || []) {
    const m = pattern.exec(msg);
    if (m) return render(m);
  }
  return msg;
}

setLanguage(null);
