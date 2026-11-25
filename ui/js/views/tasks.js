// Tasks: a filterable list and a drag-and-drop board over the same data.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, openTask, openMenu, openModal, updateTask } from '../store.js';
import { avatars, avatar, statusPill, statusIcon, priorityPill, dueLabel, pullChip, labelTag, empty } from '../components.js';
import * as U from '../util.js';

const GROUPS = [
  ['active', 'Active'], ['todo', 'To do'], ['in_progress', 'In progress'], ['in_review', 'In review'], ['done', 'Done'],
  ['overdue', 'Overdue'], ['week', 'Due this week'], ['all', 'All'],
];
const SORTS = [['smart', 'Status, then due date'], ['due', 'Due date'], ['priority', 'Priority'], ['updated', 'Last updated'], ['number', 'Newest first']];
const STATUS_ORDER = ['in_progress', 'in_review', 'todo', 'done', 'canceled'];

function inGroup(t, g) {
  switch (g) {
    case 'active': return U.isOpen(t);
    case 'all': return true;
    case 'overdue': return U.isOpen(t) && !!t.due && U.daysUntil(t.due) < 0;
    case 'week': return U.isOpen(t) && !!t.due && U.daysUntil(t.due) >= 0 && U.daysUntil(t.due) <= 7;
    default: return t.status === g;
  }
}

/** Tasks after search / person / priority filters (before the status group). */
function narrowed() {
  const ui = state.tasksUi;
  const me = state.snap.viewer.login.toLowerCase();
  const q = ui.search.trim().toLowerCase();
  return state.snap.tasks.filter((t) => {
    if (ui.mine && !t.assignees.some((a) => a.login.toLowerCase() === me)) return false;
    if (ui.priority && (t.priority || 'none') !== ui.priority) return false;
    if (ui.assignee === 'none' && t.assignees.length) return false;
    if (ui.assignee && ui.assignee !== 'none' && !t.assignees.some((a) => a.login === ui.assignee)) return false;
    if (!q) return true;
    return t.title.toLowerCase().includes(q) || ('#' + t.number).includes(q) || (t.body || '').toLowerCase().includes(q)
      || t.assignees.some((a) => a.login.toLowerCase().includes(q) || (a.name || '').toLowerCase().includes(q))
      || t.labels.some((l) => l.name.toLowerCase().includes(q));
  });
}

const dueKey = (t) => t.due || '9999-99-99';
function sorted(list) {
  const by = state.tasksUi.sort;
  const cmp = {
    smart: (a, b) => STATUS_ORDER.indexOf(a.status) - STATUS_ORDER.indexOf(b.status) || dueKey(a).localeCompare(dueKey(b)) || U.prioRank(a.priority) - U.prioRank(b.priority) || b.number - a.number,
    due: (a, b) => dueKey(a).localeCompare(dueKey(b)) || b.number - a.number,
    priority: (a, b) => U.prioRank(a.priority) - U.prioRank(b.priority) || dueKey(a).localeCompare(dueKey(b)),
    updated: (a, b) => b.updatedAt.localeCompare(a.updatedAt),
    number: (a, b) => b.number - a.number,
  }[by];
  return [...list].sort(cmp);
}

export function statusMenu(anchor, task) {
  openMenu(anchor, U.STATUSES.map((s) => ({ label: U.STATUS_LABEL[s], node: statusIcon(s), checked: task.status === s, onClick: () => { if (task.status !== s) updateTask(task.number, { status: s }); } })));
}

export function priorityMenu(anchor, task) {
  const items = U.PRIORITIES.map((p) => ({ label: U.PRIORITY_LABEL[p], icon: 'flag', checked: task.priority === p, onClick: () => { if (task.priority !== p) updateTask(task.number, { priority: p }); } }));
  items.push('sep', { label: 'No priority', icon: 'minus', checked: !task.priority, onClick: () => { if (task.priority) updateTask(task.number, { priority: null }); } });
  openMenu(anchor, items);
}

export function assigneeMenu(anchor, task) {
  const current = new Set(task.assignees.map((a) => a.login));
  const toggle = (login) => {
    const next = new Set(current);
    if (next.has(login)) next.delete(login);
    else next.add(login);
    updateTask(task.number, { assignees: [...next] });
  };
  const items = state.snap.members.map((m) => ({ label: U.displayName(m), sub: '@' + m.login, node: avatar(m, 24), checked: current.has(m.login), onClick: () => toggle(m.login) }));
  if (current.size) items.push('sep', { label: 'Unassign everyone', icon: 'x', onClick: () => updateTask(task.number, { assignees: [] }) });
  openMenu(anchor, items, { search: state.snap.members.length > 7, placeholder: 'Find a person…', minWidth: 240 });
}

function taskMeta(t) {
  return h('span', { class: 't-meta' },
    t.labels.slice(0, 2).map(labelTag),
    t.pulls.map((p) => pullChip(p)),
    t.pulls.length === 0 && t.branches.length ? h('span', { class: 'pr-chip', 'data-tip': 'Branch: ' + t.branches[0] }, icon('branch', 13), 'branch') : null,
    t.commentCount ? h('span', { class: 'cmt', 'data-tip': U.plural(t.commentCount, 'comment') }, icon('comment', 13), String(t.commentCount)) : null);
}

function listView(list) {
  if (!list.length) return h('div', { class: 'well' }, empty('tasks', 'No tasks here', state.tasksUi.search ? 'Nothing matches your search.' : 'Try another filter, or add a task.', h('button', { class: 'btn primary', type: 'button', style: { 'margin-top': '10px' }, onclick: () => openModal({ kind: 'newTask' }) }, icon('plus', 16), 'New task')));
  return h('table', { class: 'tasks' },
    h('thead', null, h('tr', null, h('th', null, 'Task'), h('th', { style: { width: '120px' } }, 'People'), h('th', { style: { width: '120px' } }, 'Due'), h('th', { style: { width: '120px' } }, 'Priority'), h('th', { style: { width: '150px' } }, 'Status'))),
    h('tbody', null, list.map((t) => h('tr', { key: 't' + t.number, class: U.isOpen(t) ? null : 'is-done', onclick: () => openTask(t.number) },
      h('td', null, h('div', { class: 't-title' }, h('span', { class: 't-num' }, '#' + t.number), h('span', { class: 'truncate' }, t.title), taskMeta(t))),
      h('td', null, h('button', { class: 'prop-btn', type: 'button', 'aria-label': 'Change assignees', onclick: (e) => { e.stopPropagation(); assigneeMenu(e.currentTarget, t); } }, avatars(t.assignees, 24))),
      h('td', null, dueLabel(t)),
      h('td', null, priorityPill(t.priority, (e) => { e.stopPropagation(); priorityMenu(e.currentTarget, t); })),
      h('td', null, statusPill(t.status, (e) => { e.stopPropagation(); statusMenu(e.currentTarget, t); }))))));
}

// ---- board -----------------------------------------------------------------------

const LANES = ['todo', 'in_progress', 'in_review', 'done'];
let dragging = null;

function card(t) {
  return h('div', {
    class: 'tcard', key: 'c' + t.number, draggable: 'true', tabindex: 0, role: 'button',
    onclick: () => openTask(t.number),
    onkeydown: (e) => { if (e.key === 'Enter') openTask(t.number); },
    ondragstart: (e) => { dragging = t.number; e.dataTransfer.setData('text/plain', String(t.number)); e.dataTransfer.effectAllowed = 'move'; e.currentTarget.classList.add('dragging'); },
    ondragend: (e) => { dragging = null; e.currentTarget.classList.remove('dragging'); document.querySelectorAll('.lane.over').forEach((l) => l.classList.remove('over')); },
  },
    h('div', { class: 'ttl' }, h('span', { class: 'muted num' }, '#' + t.number + ' '), t.title),
    h('div', { class: 'foot' },
      t.priority ? priorityPill(t.priority) : null,
      t.due ? dueLabel(t) : null,
      t.pulls.map((p) => pullChip(p)),
      t.commentCount ? h('span', { class: 'cmt' }, icon('comment', 13), String(t.commentCount)) : null,
      avatars(t.assignees, 24)));
}

function boardView(list) {
  const recent = (t) => t.status !== 'done' || !t.closedAt || Date.now() - new Date(t.closedAt).getTime() < 14 * 86400000;
  return h('div', { class: 'board' }, LANES.map((status) => {
    const all = sorted(list.filter((t) => t.status === status));
    const shown = all.filter(recent);
    return h('div', {
      class: 'lane', key: 'lane-' + status,
      ondragover: (e) => { if (dragging != null) { e.preventDefault(); e.currentTarget.classList.add('over'); } },
      ondragleave: (e) => { if (!e.currentTarget.contains(e.relatedTarget)) e.currentTarget.classList.remove('over'); },
      ondrop: (e) => {
        e.preventDefault();
        e.currentTarget.classList.remove('over');
        const number = Number(e.dataTransfer.getData('text/plain')) || dragging;
        const t = state.snap.tasks.find((x) => x.number === number);
        if (t && t.status !== status) updateTask(number, { status });
      },
    },
      h('div', { class: 'lane-head' }, statusPill(status), h('span', { class: 'n' }, String(all.length)),
        status !== 'done' ? h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': `New task in ${U.STATUS_LABEL[status]}`, onclick: () => openModal({ kind: 'newTask', status }) }, icon('plus', 15)) : null),
      shown.map(card),
      all.length > shown.length ? h('div', { class: 'lane-more' }, `${all.length - shown.length} older, shown in the list view`) : null,
      shown.length === 0 ? h('div', { class: 'lane-more' }, 'Nothing here') : null);
  }));
}

function memberMenu(anchor) {
  const ui = state.tasksUi;
  const pick = (v) => { ui.assignee = v; render(); };
  const items = [{ label: 'Everyone', icon: 'team', checked: !ui.assignee, onClick: () => pick(null) }, { label: 'Unassigned', icon: 'user', checked: ui.assignee === 'none', onClick: () => pick('none') }, 'sep'];
  for (const m of state.snap.members) items.push({ label: U.displayName(m), sub: '@' + m.login, node: avatar(m, 24), checked: ui.assignee === m.login, onClick: () => pick(m.login) });
  openMenu(anchor, items, { search: state.snap.members.length > 7, minWidth: 230 });
}

export function tasksView() {
  const ui = state.tasksUi;
  const base = narrowed();
  const board = ui.mode === 'board';
  const list = sorted(board ? base : base.filter((t) => inGroup(t, ui.group)));
  const who = ui.assignee === 'none' ? 'Unassigned' : ui.assignee ? U.displayName(state.snap.members.find((m) => m.login === ui.assignee) || { login: ui.assignee }) : 'Everyone';

  return h('div', null,
    h('div', { class: 'toolbar' },
      h('div', { class: 'seg', role: 'tablist' },
        h('button', { class: cx(!board && 'on'), type: 'button', onclick: () => { ui.mode = 'list'; render(); } }, icon('list', 15), 'List'),
        h('button', { class: cx(board && 'on'), type: 'button', onclick: () => { ui.mode = 'board'; render(); } }, icon('board', 15), 'Board')),
      h('span', { class: 'muted', style: { 'margin-left': '6px' } }, `${U.plural(list.length, 'task')}${ui.search ? ` matching "${ui.search}"` : ''}`),
      h('span', { class: 'spacer' }),
      !board ? h('button', { class: 'btn sm', type: 'button', onclick: (e) => openMenu(e.currentTarget, SORTS.map(([k, label]) => ({ label, checked: ui.sort === k, onClick: () => { ui.sort = k; render(); } })), { alignRight: true }) }, icon('arrowDown', 14), 'Sort: ' + SORTS.find((s) => s[0] === ui.sort)[1]) : null),
    h('div', { class: 'filters' },
      !board ? GROUPS.map(([g, label]) => h('button', { class: cx('chip', ui.group === g && 'on'), type: 'button', key: 'g-' + g, onclick: () => { ui.group = g; render(); } }, label, h('span', { class: 'n' }, String(base.filter((t) => inGroup(t, g)).length)))) : null,
      !board ? h('span', { class: 'divider' }) : null,
      h('button', { class: cx('chip', ui.mine && 'on'), type: 'button', onclick: () => { ui.mine = !ui.mine; render(); } }, icon('user', 14), 'My tasks'),
      h('button', { class: cx('chip', ui.assignee && 'on'), type: 'button', onclick: (e) => memberMenu(e.currentTarget) }, icon('team', 14), who, icon('chevDown', 12)),
      h('button', { class: cx('chip', ui.priority && 'on'), type: 'button', onclick: (e) => openMenu(e.currentTarget, [{ label: 'Any priority', checked: !ui.priority, onClick: () => { ui.priority = null; render(); } }, 'sep', ...U.PRIORITIES.map((p) => ({ label: U.PRIORITY_LABEL[p], icon: 'flag', checked: ui.priority === p, onClick: () => { ui.priority = p; render(); } })), { label: 'No priority', icon: 'minus', checked: ui.priority === 'none', onClick: () => { ui.priority = 'none'; render(); } }]) },
        icon('flag', 14), ui.priority ? (U.PRIORITY_LABEL[ui.priority] || 'No priority') : 'Priority', icon('chevDown', 12)),
      ui.mine || ui.assignee || ui.priority || ui.search ? h('button', { class: 'btn quiet sm', type: 'button', onclick: () => { ui.mine = false; ui.assignee = null; ui.priority = null; ui.search = ''; render(); } }, icon('x', 14), 'Clear') : null),
    board ? boardView(base) : listView(list));
}
