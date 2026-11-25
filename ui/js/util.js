// Dates, labels and small helpers shared by the views.
import { t, tn, N, locale, tError } from './i18n.js';

export const STATUSES = ['todo', 'in_progress', 'in_review', 'done', 'canceled'];
const STATUS_LABEL = { todo: N('To do'), in_progress: N('In progress'), in_review: N('In review'), done: N('Done'), canceled: N('Canceled') };
export const PRIORITIES = ['urgent', 'high', 'medium', 'low'];
const PRIORITY_LABEL = { urgent: N('Urgent'), high: N('High'), medium: N('Medium'), low: N('Low') };
const ROLE_LABEL = { admin: N('Admin'), maintain: N('Maintainer'), write: N('Member'), triage: N('Triage'), read: N('Read only'), member: N('Member') };
const PULL_LABEL = { open: N('Open'), draft: N('Draft'), merged: N('Merged'), closed: N('Closed') };

export const statusLabel = (s) => t(STATUS_LABEL[s] || s);
export const priorityLabel = (p) => t(PRIORITY_LABEL[p] || p);
export const roleLabel = (r) => t(ROLE_LABEL[r] || r);
export const pullLabel = (kind) => t(PULL_LABEL[kind] || kind);

export const isOpen = (task) => task.status === 'todo' || task.status === 'in_progress' || task.status === 'in_review';

const pad = (n) => String(n).padStart(2, '0');
/** Local calendar day of a Date as YYYY-MM-DD. */
export const ymd = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
/** "2026-11-25" -> a Date at local midnight. */
export const fromYmd = (s) => {
  const [y, m, d] = s.split('-').map(Number);
  return new Date(y, m - 1, d);
};
export const today = () => {
  const n = new Date();
  return new Date(n.getFullYear(), n.getMonth(), n.getDate());
};
export const addDays = (d, n) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
/** Whole days from today to a YYYY-MM-DD date (negative = past). */
export const daysUntil = (s) => Math.round((fromYmd(s) - today()) / 86400000);

const capital = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);
export const shortDate = (d) => d.toLocaleDateString(locale(), { month: 'short', day: 'numeric' });
export const weekday = (d) => capital(d.toLocaleDateString(locale(), { weekday: 'long' }));
export const longDate = (d) => d.toLocaleDateString(locale(), { year: 'numeric', month: 'short', day: 'numeric' });
export const dateTime = (iso) => new Date(iso).toLocaleString(locale());
export const clock = (iso) => new Date(iso).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' });

/** A due date for display: { text, tone } where tone is over | soon | '' */
export function dueInfo(due, open = true) {
  if (!due) return null;
  const n = daysUntil(due);
  const d = fromYmd(due);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  let text = sameYear ? shortDate(d) : longDate(d);
  if (n === 0) text = t('Today');
  else if (n === 1) text = t('Tomorrow');
  else if (n === -1) text = t('Yesterday');
  const tone = !open ? '' : n < 0 ? 'over' : n <= 1 ? 'soon' : '';
  return { text, tone, days: n };
}

/** "2 days ago" / "hace 2 días" from an ISO timestamp. */
export function ago(iso) {
  if (!iso) return '';
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 45) return t('just now');
  const rel = new Intl.RelativeTimeFormat(locale(), { numeric: 'auto' });
  if (s < 3600) return rel.format(-Math.max(1, Math.round(s / 60)), 'minute');
  if (s < 86400) return rel.format(-Math.round(s / 3600), 'hour');
  if (s < 86400 * 30) return rel.format(-Math.round(s / 86400), 'day');
  if (s < 86400 * 365) return rel.format(-Math.round(s / (86400 * 30)), 'month');
  return rel.format(-Math.round(s / (86400 * 365)), 'year');
}

export const dayOf = (iso) => ymd(new Date(iso));
export const fmt = (n) => Number(n || 0).toLocaleString(locale());
export const pct = (a, b) => (b > 0 ? Math.round((a / b) * 100) : 0);
export const displayName = (u) => (u && (u.name || u.login)) || t('Someone');
export const initials = (u) => {
  const name = ((u && (u.name || u.login)) || '?').replace(/[^\p{L}\p{N} ]/gu, ' ').trim();
  const parts = name.split(/\s+/).filter(Boolean);
  return ((parts[0]?.[0] || '?') + (parts.length > 1 ? parts[parts.length - 1][0] : '')).toUpperCase();
};
export const avatarUrl = (url, size = 64) => {
  if (!url || url.startsWith('data:')) return url || '';
  return url + (url.includes('?') ? '&' : '?') + 's=' + size;
};

/** Priority rank for sorting: urgent first, none last. */
export const prioRank = (p) => {
  const i = PRIORITIES.indexOf(p);
  return i < 0 ? 9 : i;
};

export function greeting() {
  const hr = new Date().getHours();
  return hr < 5 ? t('Good evening') : hr < 12 ? t('Good morning') : hr < 18 ? t('Good afternoon') : t('Good evening');
}

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** The state of a pull request in one word. */
export function pullKind(p) {
  if (p.state === 'merged') return 'merged';
  if (p.state === 'closed') return 'closed';
  return p.isDraft ? 'draft' : 'open';
}

/** A core error as text in the current language. */
export const errorText = (e) => tError(e);

// Counted nouns used on several screens.
export const nTasks = (n) => tn(n, '{n} task', '{n} tasks');
export const nCommits = (n) => tn(n, '{n} commit', '{n} commits');
export const nDays = (n) => tn(n, '{n} day', '{n} days');
