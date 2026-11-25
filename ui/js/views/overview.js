// Overview: what is on your plate and what the team is doing right now.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, go, openTask, openModal } from '../store.js';
import { avatar, statusIcon, priorityPill, dueLabel, pullChip, pullPill, empty } from '../components.js';
import { openUrl } from '../api.js';
import { t, tn } from '../i18n.js';
import * as U from '../util.js';

const dueKey = (task) => task.due || '9999-99-99';
const byUrgency = (a, b) => dueKey(a).localeCompare(dueKey(b)) || U.prioRank(a.priority) - U.prioRank(b.priority) || b.number - a.number;

function showTasks(preset) {
  Object.assign(state.tasksUi, { mode: 'list', mine: false, assignee: null, priority: null, search: '', ...preset });
  go('tasks');
}

function kpi(iconName, value, label, onClick, bad) {
  return h('button', { class: cx('kpi', bad && value > 0 && 'bad'), type: 'button', onclick: onClick },
    h('span', { class: 'ic' }, icon(iconName, 17)), h('b', null, U.fmt(value)), h('span', null, label));
}

function taskRow(task, withPeople) {
  return h('div', { class: 'list-row', key: 'r' + task.number, role: 'button', tabindex: 0, onclick: () => openTask(task.number), onkeydown: (e) => { if (e.key === 'Enter') openTask(task.number); } },
    statusIcon(task.status),
    h('span', { class: 'muted num' }, '#' + task.number),
    h('span', { class: 'ttl truncate' }, task.title),
    h('span', { class: 'right' },
      task.pulls.map((p) => pullChip(p)),
      task.priority ? priorityPill(task.priority) : null,
      task.due ? dueLabel(task) : null,
      withPeople ? task.assignees.slice(0, 2).map((a) => avatar(a, 24)) : null));
}

/** "2 in progress · 1 to do" for one person. */
function workload(doing, waiting) {
  const parts = [];
  if (doing) parts.push(t('{n} in progress', { n: doing }));
  if (waiting) parts.push(t('{n} to do', { n: waiting }));
  return parts.length ? parts.join(' · ') : t('nothing assigned');
}

export function overviewView() {
  const snap = state.snap;
  const me = snap.viewer.login.toLowerCase();
  const open = snap.tasks.filter(U.isOpen);
  const mine = open.filter((x) => x.assignees.some((a) => a.login.toLowerCase() === me)).sort(byUrgency);
  const overdue = open.filter((x) => x.due && U.daysUntil(x.due) < 0);
  const week = open.filter((x) => x.due && U.daysUntil(x.due) >= 0 && U.daysUntil(x.due) <= 7);
  const myOverdue = mine.filter((x) => x.due && U.daysUntil(x.due) < 0).length;
  const first = U.displayName(snap.viewer).split(' ')[0];
  const line = mine.length === 0
    ? t('Nothing is assigned to you right now.')
    : tn(mine.length, '{n} task is on your plate.', '{n} tasks are on your plate.') + (myOverdue ? ' ' + tn(myOverdue, '{n} is overdue.', '{n} are overdue.') : '');
  const openPulls = snap.pulls.filter((p) => p.state === 'open');
  const unassigned = open.filter((x) => x.assignees.length === 0).sort(byUrgency);

  return h('div', null,
    h('div', { class: 'hello' },
      avatar(snap.viewer, 40),
      h('div', null, h('h2', null, `${U.greeting()}, ${first}`), h('p', null, line)),
      h('div', { class: 'right' }, h('button', { class: 'btn primary', type: 'button', onclick: () => openModal({ kind: 'newTask' }) }, icon('plus', 16), t('New task')))),

    h('div', { class: 'kpis' },
      kpi('tasks', open.length, t('Active tasks'), () => showTasks({ group: 'active' })),
      kpi('alert', overdue.length, t('Overdue'), () => showTasks({ group: 'overdue' }), true),
      kpi('calendar', week.length, t('Due this week'), () => showTasks({ group: 'week' })),
      kpi('team', snap.members.length, snap.members.length === 1 ? t('Team member') : t('Team members'), () => go('team'))),

    h('div', { class: 'ov-grid' },
      h('div', { class: 'col', style: { gap: '14px' } },
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, t('My work'), h('span', { class: 'right' }, h('button', { class: 'btn quiet sm', type: 'button', onclick: () => showTasks({ group: 'active', mine: true }) }, t('View all'), icon('chevRight', 14)))),
          mine.length ? mine.slice(0, 8).map((x) => taskRow(x, false)) : empty('checkCircle', t('All clear'), t('Tasks assigned to you show up here.'))),
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, t('What the team is working on')),
          snap.members.map((m) => {
            const doing = open.filter((x) => x.status !== 'todo' && x.assignees.some((a) => a.login === m.login)).sort(byUrgency);
            const waiting = open.filter((x) => x.status === 'todo' && x.assignees.some((a) => a.login === m.login)).length;
            return [
              h('div', { class: 'person-head', key: 'ph-' + m.login }, avatar(m, 28), h('span', null, U.displayName(m)), h('span', { class: 'n' }, workload(doing.length, waiting))),
              doing.map((x) => taskRow(x, false)),
              doing.length === 0 ? h('div', { class: 'list-row', key: 'idle-' + m.login, style: { cursor: 'default', color: 'var(--ink-3)' } }, t('Nothing in progress')) : null,
            ];
          }))),
      h('div', { class: 'col', style: { gap: '14px' } },
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, t('Coming up'), h('span', { class: 'right' }, h('button', { class: 'btn quiet sm', type: 'button', onclick: () => go('agenda') }, t('Agenda'), icon('chevRight', 14)))),
          [...overdue, ...week].sort(byUrgency).slice(0, 7).map((x) => taskRow(x, true)),
          overdue.length + week.length === 0 ? empty('calendar', t('Nothing due this week')) : null),
        h('div', { class: 'panel' },
          h('div', { class: 'panel-head' }, t('Open pull requests'), h('span', { class: 'right muted', style: { 'font-weight': '500', 'font-size': '13px' } }, String(snap.repo.openPulls))),
          openPulls.slice(0, 6).map((p) => h('div', { class: 'list-row', key: 'p' + p.number, role: 'button', tabindex: 0, onclick: () => openUrl(p.url) },
            avatar(p.author, 24), h('span', { class: 'ttl truncate' }, p.title), h('span', { class: 'right' }, pullPill(p)))),
          openPulls.length === 0 ? empty('pr', t('No open pull requests')) : null),
        unassigned.length
          ? h('div', { class: 'panel' },
              h('div', { class: 'panel-head' }, t('Nobody is on these yet'), h('span', { class: 'right' }, h('button', { class: 'btn quiet sm', type: 'button', onclick: () => showTasks({ group: 'active', assignee: 'none' }) }, t('View all'), icon('chevRight', 14)))),
              unassigned.slice(0, 5).map((x) => taskRow(x, false)))
          : null)));
}
