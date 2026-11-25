// Agenda: meetings from your calendars and the tasks that are due, day by day.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, openTask, go, openModal, loadAgenda, AGENDA_DAYS } from '../store.js';
import { avatar, statusIcon, priorityPill, pullChip, empty, spinner } from '../components.js';
import { openUrl } from '../api.js';
import * as U from '../util.js';

const clock = (iso) => new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

/** The local days (YYYY-MM-DD) an event belongs to. */
function daysOf(ev) {
  if (!ev.allDay) return [U.ymd(new Date(ev.start))];
  const days = [];
  for (let d = U.fromYmd(ev.start); U.ymd(d) < ev.end && days.length < 60; d = U.addDays(d, 1)) days.push(U.ymd(d));
  return days;
}

function eventRow(ev, withDate) {
  const when = ev.allDay ? 'All day' : `${clock(ev.start)} – ${clock(ev.end)}`;
  const date = withDate ? U.shortDate(ev.allDay ? U.fromYmd(withDate) : new Date(ev.start)) + ' · ' : '';
  return h('div', { class: 'list-row event', key: 'e' + ev.id + (withDate || '') },
    h('span', { class: 'ev-ic' }, icon('calendar', 15)),
    h('span', { class: 'ev-time num' }, date + when),
    h('span', { class: 'ttl truncate', 'data-tip': ev.title }, ev.title),
    ev.recurring ? h('span', { class: 'muted', 'data-tip': 'Repeats' }, icon('refresh', 13)) : null,
    ev.location ? h('span', { class: 'muted truncate', style: { 'max-width': '220px' }, 'data-tip': ev.location }, ev.location) : null,
    h('span', { class: 'right' },
      ev.link ? h('button', { class: 'btn sm', type: 'button', 'data-tip': 'Open the meeting link', onclick: () => openUrl(ev.link) }, icon('external', 13), 'Join') : null,
      state.cal.list.length > 1 ? h('span', { class: 'tag' }, ev.calendar) : null));
}

function taskRow(t) {
  return h('div', { class: 'list-row', key: 'a' + t.number, role: 'button', tabindex: 0, onclick: () => openTask(t.number), onkeydown: (e) => { if (e.key === 'Enter') openTask(t.number); } },
    statusIcon(t.status),
    h('span', { class: 'muted num' }, '#' + t.number),
    h('span', { class: 'ttl truncate' }, t.title),
    h('span', { class: 'right' }, h('span', { class: 'tag' }, 'Due'), t.pulls.map((p) => pullChip(p)), t.priority ? priorityPill(t.priority) : null, t.assignees.length ? t.assignees.slice(0, 3).map((a) => avatar(a, 24)) : avatar(null, 24)));
}

function section(key, title, sub, rows, over) {
  if (!rows.length) return null;
  return h('section', { class: 'day', key },
    h('div', { class: cx('day-head', over && 'over') }, h('b', null, title), h('span', null, sub)),
    h('div', { class: 'panel' }, rows));
}

function calendarBar() {
  const c = state.cal;
  return h('div', { class: 'toolbar' },
    c.list.length
      ? [h('span', { class: 'soft row', style: { gap: '6px' } }, icon('calendar', 15), c.list.map((x) => x.name).join(', ')),
         c.loadedAt ? h('span', { class: 'muted' }, '· meetings for the next two weeks, read ' + U.ago(new Date(c.loadedAt).toISOString())) : null]
      : h('span', { class: 'muted' }, 'No calendar connected: showing task due dates only.'),
    h('span', { class: 'spacer' }),
    c.list.length ? h('button', { class: 'icon-btn', type: 'button', 'aria-label': 'Read the calendars again', 'data-tip': 'Read the calendars again', onclick: () => loadAgenda(true) }, c.loading ? spinner() : icon('refresh', 16)) : null,
    h('button', { class: c.list.length ? 'btn' : 'btn primary', type: 'button', onclick: () => openModal({ kind: 'connectCalendar' }) }, icon('plus', 15), c.list.length ? 'Add a calendar' : 'Connect Google Calendar'));
}

export function agendaView() {
  const c = state.cal;
  const open = state.snap.tasks.filter(U.isOpen);
  const dated = open.filter((t) => t.due).sort((a, b) => a.due.localeCompare(b.due) || U.prioRank(a.priority) - U.prioRank(b.priority));
  const undated = open.length - dated.length;
  const t0 = U.today();

  const byDay = new Map();
  for (const ev of c.events) for (const d of daysOf(ev)) (byDay.get(d) || byDay.set(d, []).get(d)).push(ev);

  const sections = [];
  sections.push(section('over', 'Overdue', '', dated.filter((t) => U.daysUntil(t.due) < 0).map(taskRow), true));
  for (let i = 0; i <= 6; i++) {
    const d = U.addDays(t0, i);
    const key = U.ymd(d);
    const rows = [...(byDay.get(key) || []).map((ev) => eventRow(ev)), ...dated.filter((t) => t.due === key).map(taskRow)];
    sections.push(section(key, i === 0 ? 'Today' : i === 1 ? 'Tomorrow' : U.weekday(d), U.shortDate(d), rows));
  }
  const nextWeek = [];
  for (let i = 7; i < AGENDA_DAYS; i++) {
    const key = U.ymd(U.addDays(t0, i));
    for (const ev of byDay.get(key) || []) nextWeek.push(eventRow(ev, key));
  }
  nextWeek.push(...dated.filter((t) => U.daysUntil(t.due) >= 7 && U.daysUntil(t.due) < AGENDA_DAYS).map(taskRow));
  sections.push(section('next', 'Next week', `${U.shortDate(U.addDays(t0, 7))} – ${U.shortDate(U.addDays(t0, AGENDA_DAYS - 1))}`, nextWeek));
  sections.push(section('later', 'Later', 'tasks only', dated.filter((t) => U.daysUntil(t.due) >= AGENDA_DAYS).map(taskRow)));
  const any = sections.some(Boolean);

  return h('div', { class: 'agenda' },
    calendarBar(),
    c.error ? h('div', { class: 'banner err' }, icon('alert', 17), h('span', null, c.error)) : null,
    c.problems.map((p) => h('div', { class: 'banner warn', key: 'p' + p.calendarId }, icon('alert', 17), h('span', null, h('b', null, p.calendar + ': '), p.message, p.showingOldCopy ? ' Showing the last copy that was read.' : ''))),
    any ? sections : h('div', { class: 'well' }, empty('calendar', 'Nothing on the agenda', c.list.length ? 'No meetings in the next two weeks and no task has a due date.' : 'Give a task a due date, or connect your calendar to see your meetings here.')),
    undated
      ? h('div', { class: 'row', style: { margin: '4px 6px 16px' } }, h('span', { class: 'muted' }, `${U.plural(undated, 'open task has', 'open tasks have')} no due date.`),
          h('button', { class: 'btn quiet sm', type: 'button', onclick: () => { Object.assign(state.tasksUi, { mode: 'list', group: 'active', sort: 'due', mine: false, assignee: null, priority: null, search: '' }); go('tasks'); } }, 'Show them', icon('chevRight', 14)))
      : null,
    c.list.length === 0
      ? h('div', { class: 'note-card' }, icon('calendar', 17),
          h('span', null, h('b', null, 'See your meetings here. '), 'Connect a Google Calendar with its private link. It is read-only: Headway can show your events but cannot change them. The link stays on this computer.'))
      : null);
}
