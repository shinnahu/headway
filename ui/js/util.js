// Dates, labels and small helpers shared by the views.

export const STATUSES = ['todo', 'in_progress', 'in_review', 'done', 'canceled'];
export const STATUS_LABEL = { todo: 'To do', in_progress: 'In progress', in_review: 'In review', done: 'Done', canceled: 'Canceled' };
export const PRIORITIES = ['urgent', 'high', 'medium', 'low'];
export const PRIORITY_LABEL = { urgent: 'Urgent', high: 'High', medium: 'Medium', low: 'Low' };
export const ROLE_LABEL = { admin: 'Admin', maintain: 'Maintainer', write: 'Member', triage: 'Triage', read: 'Read only', member: 'Member' };

export const isOpen = (t) => t.status === 'todo' || t.status === 'in_progress' || t.status === 'in_review';

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

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
export const shortDate = (d) => `${MONTHS[d.getMonth()]} ${d.getDate()}`;
export const weekday = (d) => WEEKDAYS[d.getDay()];
export const longDate = (d) => `${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`;

/** A due date for display: { text, tone } where tone is over | soon | '' */
export function dueInfo(due, open = true) {
  if (!due) return null;
  const n = daysUntil(due);
  const d = fromYmd(due);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  let text = sameYear ? shortDate(d) : longDate(d);
  if (n === 0) text = 'Today';
  else if (n === 1) text = 'Tomorrow';
  else if (n === -1) text = 'Yesterday';
  const tone = !open ? '' : n < 0 ? 'over' : n <= 1 ? 'soon' : '';
  return { text, tone, days: n };
}

/** "2 days ago", "just now" ... from an ISO timestamp. */
export function ago(iso) {
  if (!iso) return '';
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 45) return 'just now';
  if (s < 90) return 'a minute ago';
  if (s < 3600) return `${Math.round(s / 60)} minutes ago`;
  if (s < 5400) return 'an hour ago';
  if (s < 86400) return `${Math.round(s / 3600)} hours ago`;
  if (s < 129600) return 'yesterday';
  if (s < 86400 * 30) return `${Math.round(s / 86400)} days ago`;
  if (s < 86400 * 45) return 'a month ago';
  if (s < 86400 * 365) return `${Math.round(s / (86400 * 30))} months ago`;
  return `${Math.round(s / (86400 * 365))} years ago`.replace(/^1 years/, 'a year');
}

export const dayOf = (iso) => ymd(new Date(iso));
export const fmt = (n) => Number(n || 0).toLocaleString('en-US');
export const pct = (a, b) => (b > 0 ? Math.round((a / b) * 100) : 0);
export const plural = (n, one, many) => `${fmt(n)} ${n === 1 ? one : many || one + 's'}`;
export const displayName = (u) => (u && (u.name || u.login)) || 'Someone';
export const initials = (u) => {
  const name = displayName(u).replace(/[^\p{L}\p{N} ]/gu, ' ').trim();
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
  return hr < 5 ? 'Good evening' : hr < 12 ? 'Good morning' : hr < 18 ? 'Good afternoon' : 'Good evening';
}

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** Checks state of a pull request in one word for display. */
export function pullKind(p) {
  if (p.state === 'merged') return 'merged';
  if (p.state === 'closed') return 'closed';
  return p.isDraft ? 'draft' : 'open';
}
export const PULL_LABEL = { open: 'Open', draft: 'Draft', merged: 'Merged', closed: 'Closed' };

export function errorText(e) {
  if (!e) return 'Something went wrong.';
  if (typeof e === 'string') return e;
  return e.message || String(e);
}
