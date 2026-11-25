// Tasks: a filterable list and a drag-and-drop board over the same data.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, openTask, openMenu, openModal, updateTask } from '../store.js';
import { avatars, avatar, statusPill, statusIcon, priorityPill, dueLabel, pullChip, labelTag, empty } from '../components.js';
import { t, tn, N } from '../i18n.js';
import * as U from '../util.js';

const GROUPS = [
  ['active', N('Active')], ['todo', N('To do')], ['in_progress', N('In progress')], ['in_review', N('In review')], ['done', N('Done')],
  ['overdue', N('Overdue')], ['week', N('Due this week')], ['all', N('All')],
];
const SORTS = [['smart', N('Status, then due date')], ['due', N('Due date')], ['priority', N('Priority')], ['updated', N('Last updated')], ['number', N('Newest first')]];
const STATUS_ORDER = ['in_progress', 'in_review', 'todo', 'done', 'canceled'];

function inGroup(task, g) {
  switch (g) {
    case 'active': return U.isOpen(task);
    case 'all': return true;
    case 'overdue': return U.isOpen(task) && !!task.due && U.daysUntil(task.due) < 0;
    case 'week': return U.isOpen(task) && !!task.due && U.daysUntil(task.due) >= 0 && U.daysUntil(task.due) <= 7;
    default: return task.status === g;
  }
}

/** Tasks after search / person / priority filters (before the status group). */
function narrowed() {
  const ui = state.tasksUi;
  const me = state.snap.viewer.login.toLowerCase();
  const q = ui.search.trim().toLowerCase();
  return state.snap.tasks.filter((task) => {
    if (ui.mine && !task.assignees.some((a) => a.login.toLowerCase() === me)) return false;
    if (ui.priority && (task.priority || 'none') !== ui.priority) return false;
    if (ui.assignee === 'none' && task.assignees.length) return false;
    if (ui.assignee && ui.assignee !== 'none' && !task.assignees.some((a) => a.login === ui.assignee)) return false;
    if (!q) return true;
    return task.title.toLowerCase().includes(q) || ('#' + task.number).includes(q) || (task.body || '').toLowerCase().includes(q)
      || task.assignees.some((a) => a.login.toLowerCase().includes(q) || (a.name || '').toLowerCase().includes(q))
      || task.labels.some((l) => l.name.toLowerCase().includes(q));
  });
}

const dueKey = (task) => task.due || '9999-99-99';
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
  openMenu(anchor, U.STATUSES.map((s) => ({ label: U.statusLabel(s), node: statusIcon(s), checked: task.status === s, onClick: () => { if (task.status !== s) updateTask(task.number, { status: s }); } })));
}

export function priorityMenu(anchor, task) {
  const items = U.PRIORITIES.map((p) => ({ label: U.priorityLabel(p), icon: 'flag', checked: task.priority === p, onClick: () => { if (task.priority !== p) updateTask(task.number, { priority: p }); } }));
  items.push('sep', { label: t('No priority'), icon: 'minus', checked: !task.priority, onClick: () => { if (task.priority) updateTask(task.number, { priority: null }); } });
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
  if (current.size) items.push('sep', { label: t('Unassign everyone'), icon: 'x', onClick: () => updateTask(task.number, { assignees: [] }) });
  openMenu(anchor, items, { search: state.snap.members.length > 7, placeholder: t('Find a person…'), minWidth: 240 });
}

function taskMeta(task) {
  return h('span', { class: 't-meta' },
    task.labels.slice(0, 2).map(labelTag),
    task.pulls.map((p) => pullChip(p)),
    task.pulls.length === 0 && task.branches.length ? h('span', { class: 'pr-chip', 'data-tip': t('Branch: {name}', { name: task.branches[0] }) }, icon('branch', 13), t('branch')) : null,
    task.commentCount ? h('span', { class: 'cmt', 'data-tip': tn(task.commentCount, '{n} comment', '{n} comments') }, icon('comment', 13), String(task.commentCount)) : null);
}

function listView(list) {
  if (!list.length) return h('div', { class: 'well' }, empty('tasks', t('No tasks here'), state.tasksUi.search ? t('Nothing matches your search.') : t('Try another filter, or add a task.'), h('button', { class: 'btn primary', type: 'button', style: { 'margin-top': '10px' }, onclick: () => openModal({ kind: 'newTask' }) }, icon('plus', 16), t('New task'))));
  return h('table', { class: 'tasks' },
    h('thead', null, h('tr', null, h('th', null, t('Task')), h('th', { style: { width: '120px' } }, t('People')), h('th', { style: { width: '120px' } }, t('Due')), h('th', { style: { width: '130px' } }, t('Priority')), h('th', { style: { width: '160px' } }, t('Status')))),
    h('tbody', null, list.map((task) => h('tr', { key: 't' + task.number, class: U.isOpen(task) ? null : 'is-done', onclick: () => openTask(task.number) },
      h('td', null, h('div', { class: 't-title' }, h('span', { class: 't-num' }, '#' + task.number), h('span', { class: 'truncate' }, task.title), taskMeta(task))),
      h('td', null, h('button', { class: 'prop-btn', type: 'button', 'aria-label': t('Change assignees'), onclick: (e) => { e.stopPropagation(); assigneeMenu(e.currentTarget, task); } }, avatars(task.assignees, 24))),
      h('td', null, dueLabel(task)),
      h('td', null, priorityPill(task.priority, (e) => { e.stopPropagation(); priorityMenu(e.currentTarget, task); })),
      h('td', null, statusPill(task.status, (e) => { e.stopPropagation(); statusMenu(e.currentTarget, task); }))))));
}

// ---- board -----------------------------------------------------------------------

const LANES = ['todo', 'in_progress', 'in_review', 'done'];
let dragging = null;

function card(task) {
  return h('div', {
    class: 'tcard', key: 'c' + task.number, draggable: 'true', tabindex: 0, role: 'button',
    onclick: () => openTask(task.number),
    onkeydown: (e) => { if (e.key === 'Enter') openTask(task.number); },
    ondragstart: (e) => { dragging = task.number; e.dataTransfer.setData('text/plain', String(task.number)); e.dataTransfer.effectAllowed = 'move'; e.currentTarget.classList.add('dragging'); },
    ondragend: (e) => { dragging = null; e.currentTarget.classList.remove('dragging'); document.querySelectorAll('.lane.over').forEach((l) => l.classList.remove('over')); },
  },
    h('div', { class: 'ttl' }, h('span', { class: 'muted num' }, '#' + task.number + ' '), task.title),
    h('div', { class: 'foot' },
      task.priority ? priorityPill(task.priority) : null,
      task.due ? dueLabel(task) : null,
      task.pulls.map((p) => pullChip(p)),
      task.commentCount ? h('span', { class: 'cmt' }, icon('comment', 13), String(task.commentCount)) : null,
      avatars(task.assignees, 24)));
}

function boardView(list) {
  const recent = (task) => task.status !== 'done' || !task.closedAt || Date.now() - new Date(task.closedAt).getTime() < 14 * 86400000;
  return h('div', { class: 'board' }, LANES.map((status) => {
    const all = sorted(list.filter((task) => task.status === status));
    const shown = all.filter(recent);
    return h('div', {
      class: 'lane', key: 'lane-' + status,
      ondragover: (e) => { if (dragging != null) { e.preventDefault(); e.currentTarget.classList.add('over'); } },
      ondragleave: (e) => { if (!e.currentTarget.contains(e.relatedTarget)) e.currentTarget.classList.remove('over'); },
      ondrop: (e) => {
        e.preventDefault();
        e.currentTarget.classList.remove('over');
        const number = Number(e.dataTransfer.getData('text/plain')) || dragging;
        const task = state.snap.tasks.find((x) => x.number === number);
        if (task && task.status !== status) updateTask(number, { status });
      },
    },
      h('div', { class: 'lane-head' }, statusPill(status), h('span', { class: 'n' }, String(all.length)),
        status !== 'done' ? h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': t('New task in {status}', { status: U.statusLabel(status) }), onclick: () => openModal({ kind: 'newTask', status }) }, icon('plus', 15)) : null),
      shown.map(card),
      all.length > shown.length ? h('div', { class: 'lane-more' }, tn(all.length - shown.length, '{n} older task, shown in the list view', '{n} older tasks, shown in the list view')) : null,
      shown.length === 0 ? h('div', { class: 'lane-more' }, t('Nothing here')) : null);
  }));
}

function memberMenu(anchor) {
  const ui = state.tasksUi;
  const pick = (v) => { ui.assignee = v; render(); };
  const items = [{ label: t('Everyone'), icon: 'team', checked: !ui.assignee, onClick: () => pick(null) }, { label: t('Unassigned'), icon: 'user', checked: ui.assignee === 'none', onClick: () => pick('none') }, 'sep'];
  for (const m of state.snap.members) items.push({ label: U.displayName(m), sub: '@' + m.login, node: avatar(m, 24), checked: ui.assignee === m.login, onClick: () => pick(m.login) });
  openMenu(anchor, items, { search: state.snap.members.length > 7, minWidth: 230 });
}

export function tasksView() {
  const ui = state.tasksUi;
  const base = narrowed();
  const board = ui.mode === 'board';
  const list = sorted(board ? base : base.filter((task) => inGroup(task, ui.group)));
  const who = ui.assignee === 'none' ? t('Unassigned') : ui.assignee ? U.displayName(state.snap.members.find((m) => m.login === ui.assignee) || { login: ui.assignee }) : t('Everyone');
  const sortName = t(SORTS.find((s) => s[0] === ui.sort)[1]);

  return h('div', null,
    h('div', { class: 'toolbar' },
      h('div', { class: 'seg', role: 'tablist' },
        h('button', { class: cx(!board && 'on'), type: 'button', onclick: () => { ui.mode = 'list'; render(); } }, icon('list', 15), t('List')),
        h('button', { class: cx(board && 'on'), type: 'button', onclick: () => { ui.mode = 'board'; render(); } }, icon('board', 15), t('Board'))),
      h('span', { class: 'muted', style: { 'margin-left': '6px' } }, ui.search ? t('{count} matching "{q}"', { count: U.nTasks(list.length), q: ui.search }) : U.nTasks(list.length)),
      h('span', { class: 'spacer' }),
      !board ? h('button', { class: 'btn sm', type: 'button', onclick: (e) => openMenu(e.currentTarget, SORTS.map(([k, label]) => ({ label: t(label), checked: ui.sort === k, onClick: () => { ui.sort = k; render(); } })), { alignRight: true }) }, icon('arrowDown', 14), t('Sort: {name}', { name: sortName })) : null),
    h('div', { class: 'filters' },
      !board ? GROUPS.map(([g, label]) => h('button', { class: cx('chip', ui.group === g && 'on'), type: 'button', key: 'g-' + g, onclick: () => { ui.group = g; render(); } }, t(label), h('span', { class: 'n' }, String(base.filter((task) => inGroup(task, g)).length)))) : null,
      !board ? h('span', { class: 'divider' }) : null,
      h('button', { class: cx('chip', ui.mine && 'on'), type: 'button', onclick: () => { ui.mine = !ui.mine; render(); } }, icon('user', 14), t('My tasks')),
      h('button', { class: cx('chip', ui.assignee && 'on'), type: 'button', onclick: (e) => memberMenu(e.currentTarget) }, icon('team', 14), who, icon('chevDown', 12)),
      h('button', { class: cx('chip', ui.priority && 'on'), type: 'button', onclick: (e) => openMenu(e.currentTarget, [{ label: t('Any priority'), checked: !ui.priority, onClick: () => { ui.priority = null; render(); } }, 'sep', ...U.PRIORITIES.map((p) => ({ label: U.priorityLabel(p), icon: 'flag', checked: ui.priority === p, onClick: () => { ui.priority = p; render(); } })), { label: t('No priority'), icon: 'minus', checked: ui.priority === 'none', onClick: () => { ui.priority = 'none'; render(); } }]) },
        icon('flag', 14), ui.priority ? (ui.priority === 'none' ? t('No priority') : U.priorityLabel(ui.priority)) : t('Priority'), icon('chevDown', 12)),
      ui.mine || ui.assignee || ui.priority || ui.search ? h('button', { class: 'btn quiet sm', type: 'button', onclick: () => { ui.mine = false; ui.assignee = null; ui.priority = null; ui.search = ''; render(); } }, icon('x', 14), t('Clear')) : null),
    board ? boardView(base) : listView(list));
}
