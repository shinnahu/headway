// Overview: what is on your plate and what the team is doing right now.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, go, openTask, openModal } from '../store.js';
import { avatar, statusIcon, priorityPill, dueLabel, pullChip, pullPill, empty } from '../components.js';
import { openUrl } from '../api.js';
import * as U from '../util.js';

const dueKey = (t) => t.due || '9999-99-99';
const byUrgency = (a, b) => dueKey(a).localeCompare(dueKey(b)) || U.prioRank(a.priority) - U.prioRank(b.priority) || b.number - a.number;

function showTasks(preset) {
  Object.assign(state.tasksUi, { mode: 'list', mine: false, assignee: null, priority: null, search: '', ...preset });
  go('tasks');
}

function kpi(iconName, value, label, onClick, bad) {
  return h('button', { class: cx('kpi', bad && value > 0 && 'bad'), type: 'button', onclick: onClick },
    h('span', { class: 'ic' }, icon(iconName, 17)), h('b', null, U.fmt(value)), h('span', null, label));
}

function taskRow(t, withPeople) {
  return h('div', { class: 'list-row', key: 'r' + t.number, role: 'button', tabindex: 0, onclick: () => openTask(t.number), onkeydown: (e) => { if (e.key === 'Enter') openTask(t.number); } },
    statusIcon(t.status),
    h('span', { class: 'muted num' }, '#' + t.number),
    h('span', { class: 'ttl truncate' }, t.title),
    h('span', { class: 'right' },
      t.pulls.map((p) => pullChip(p)),
      t.priority ? priorityPill(t.priority) : null,
      t.due ? dueLabel(t) : null,
      withPeople ? t.assignees.slice(0, 2).map((a) => avatar(a, 24)) : null));
}

export function overviewView() {
  const snap = state.snap;
  const me = snap.viewer.login.toLowerCase();
  const open = snap.tasks.filter(U.isOpen);
  const mine = open.filter((t) => t.assignees.some((a) => a.login.toLowerCase() === me)).sort(byUrgency);
  const overdue = open.filter((t) => t.due && U.daysUntil(t.due) < 0);
  const week = open.filter((t) => t.due && U.daysUntil(t.due) >= 0 && U.daysUntil(t.due) <= 7);
  const myOverdue = mine.filter((t) => t.due && U.daysUntil(t.due) < 0).length;
  const first = U.displayName(snap.viewer).split(' ')[0];
  const line = mine.length === 0
    ? 'Nothing is assigned to you right now.'
    : `${U.plural(mine.length, 'task is', 'tasks are')} on your plate` + (myOverdue ? `, ${myOverdue} overdue.` : '.');
  const openPulls = snap.pulls.filter((p) => p.state === 'open');
  const unassigned = open.filter((t) => t.assignees.length === 0).sort(byUrgency);

  return h('div', null,
    h('div', { class: 'hello' },
      avatar(snap.viewer, 40),
      h('div', null, h('h2', null, `${U.greeting()}, ${first}`), h('p', null, line)),
      h('div', { class: 'right' }, h('button', { class: 'btn primary', type: 'button', onclick: () => openModal({ kind: 'newTask' }) }, icon('plus', 16), 'New task'))),

    h('div', { class: 'kpis' },
      kpi('tasks', open.length, 'Active tasks', () => showTasks({ group: 'active' })),
      kpi('alert', overdue.length, 'Overdue', () => showTasks({ group: 'overdue' }), true),
      kpi('calendar', week.length, 'Due this week', () => showTasks({ group: 'week' })),
      kpi('team', snap.members.length, snap.members.length === 1 ? 'Team member' : 'Team members', () => go('team'))),

    h('div', { class: 'ov-grid' },
      h('div', { class: 'col', style: { gap: '14px' } },
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, 'My work', h('span', { class: 'right' }, h('button', { class: 'btn quiet sm', type: 'button', onclick: () => showTasks({ group: 'active', mine: true }) }, 'View all', icon('chevRight', 14)))),
          mine.length ? mine.slice(0, 8).map((t) => taskRow(t, false)) : empty('checkCircle', 'All clear', 'Tasks assigned to you show up here.')),
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, 'What the team is working on'),
          snap.members.map((m) => {
            const doing = open.filter((t) => t.status !== 'todo' && t.assignees.some((a) => a.login === m.login)).sort(byUrgency);
            const waiting = open.filter((t) => t.status === 'todo' && t.assignees.some((a) => a.login === m.login)).length;
            return [
              h('div', { class: 'person-head', key: 'ph-' + m.login }, avatar(m, 28), h('span', null, U.displayName(m)), h('span', { class: 'n' }, doing.length ? `${doing.length} in progress` + (waiting ? ` · ${waiting} to do` : '') : waiting ? `${waiting} to do` : 'nothing assigned')),
              doing.map((t) => taskRow(t, false)),
              doing.length === 0 ? h('div', { class: 'list-row', key: 'idle-' + m.login, style: { cursor: 'default', color: 'var(--ink-3)' } }, 'Nothing in progress') : null,
            ];
          }))),
      h('div', { class: 'col', style: { gap: '14px' } },
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, 'Coming up', h('span', { class: 'right' }, h('button', { class: 'btn quiet sm', type: 'button', onclick: () => go('agenda') }, 'Agenda', icon('chevRight', 14)))),
          [...overdue, ...week].sort(byUrgency).slice(0, 7).map((t) => taskRow(t, true)),
          overdue.length + week.length === 0 ? empty('calendar', 'Nothing due this week') : null),
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, 'Open pull requests', h('span', { class: 'right muted', style: { 'font-weight': '500', 'font-size': '13px' } }, String(snap.repo.openPulls))),
          openPulls.slice(0, 6).map((p) => h('div', { class: 'list-row', key: 'p' + p.number, role: 'button', tabindex: 0, onclick: () => openUrl(p.url) },
            avatar(p.author, 24), h('span', { class: 'ttl truncate' }, p.title), h('span', { class: 'right' }, pullPill(p)))),
          openPulls.length === 0 ? empty('pr', 'No open pull requests') : null),
        unassigned.length
          ? h('div', { class: 'panel' },
              h('div', { class: 'panel-head' }, 'Nobody is on these yet', h('span', { class: 'right' }, h('button', { class: 'btn quiet sm', type: 'button', onclick: () => showTasks({ group: 'active', assignee: 'none' }) }, 'View all', icon('chevRight', 14)))),
              unassigned.slice(0, 5).map((t) => taskRow(t, false)))
          : null)));
}
