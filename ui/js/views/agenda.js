// Agenda: meetings from your calendars and the tasks that are due, day by day.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, openTask, go, openModal, loadAgenda, AGENDA_DAYS } from '../store.js';
import { avatar, statusIcon, priorityPill, pullChip, empty, spinner, rich } from '../components.js';
import { openUrl } from '../api.js';
import { t, tn } from '../i18n.js';
import * as U from '../util.js';

/** The local days (YYYY-MM-DD) an event belongs to. */
function daysOf(ev) {
  if (!ev.allDay) return [U.ymd(new Date(ev.start))];
  const days = [];
  for (let d = U.fromYmd(ev.start); U.ymd(d) < ev.end && days.length < 60; d = U.addDays(d, 1)) days.push(U.ymd(d));
  return days;
}

function eventRow(ev, withDate) {
  const when = ev.allDay ? t('All day') : `${U.clock(ev.start)} – ${U.clock(ev.end)}`;
  const date = withDate ? U.shortDate(ev.allDay ? U.fromYmd(withDate) : new Date(ev.start)) + ' · ' : '';
  return h('div', { class: 'list-row event', key: 'e' + ev.id + (withDate || '') },
    h('span', { class: 'ev-ic' }, icon('calendar', 15)),
    h('span', { class: 'ev-time num' }, date + when),
    h('span', { class: 'ttl truncate', 'data-tip': ev.title }, ev.title),
    ev.recurring ? h('span', { class: 'muted', 'data-tip': t('Repeats') }, icon('refresh', 13)) : null,
    ev.location ? h('span', { class: 'muted truncate', style: { 'max-width': '220px' }, 'data-tip': ev.location }, ev.location) : null,
    h('span', { class: 'right' },
      ev.link ? h('button', { class: 'btn sm', type: 'button', 'data-tip': t('Open the meeting link'), onclick: () => openUrl(ev.link) }, icon('external', 13), t('Join')) : null,
      state.cal.list.length > 1 ? h('span', { class: 'tag' }, ev.calendar) : null));
}

function taskRow(task) {
  return h('div', { class: 'list-row', key: 'a' + task.number, role: 'button', tabindex: 0, onclick: () => openTask(task.number), onkeydown: (e) => { if (e.key === 'Enter') openTask(task.number); } },
    statusIcon(task.status),
    h('span', { class: 'muted num' }, '#' + task.number),
    h('span', { class: 'ttl truncate' }, task.title),
    h('span', { class: 'right' }, h('span', { class: 'tag' }, t('Due')), task.pulls.map((p) => pullChip(p)), task.priority ? priorityPill(task.priority) : null, task.assignees.length ? task.assignees.slice(0, 3).map((a) => avatar(a, 24)) : avatar(null, 24)));
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
         c.loadedAt ? h('span', { class: 'muted' }, t('· meetings for the next two weeks, read {when}', { when: U.ago(new Date(c.loadedAt).toISOString()) })) : null]
      : h('span', { class: 'muted' }, t('No calendar connected: showing task due dates only.')),
    h('span', { class: 'spacer' }),
    c.list.length ? h('button', { class: 'icon-btn', type: 'button', 'aria-label': t('Read the calendars again'), 'data-tip': t('Read the calendars again'), onclick: () => loadAgenda(true) }, c.loading ? spinner() : icon('refresh', 16)) : null,
    h('button', { class: c.list.length ? 'btn' : 'btn primary', type: 'button', onclick: () => openModal({ kind: 'connectCalendar' }) }, icon('plus', 15), c.list.length ? t('Add a calendar') : t('Connect Google Calendar')));
}

export function agendaView() {
  const c = state.cal;
  const open = state.snap.tasks.filter(U.isOpen);
  const dated = open.filter((x) => x.due).sort((a, b) => a.due.localeCompare(b.due) || U.prioRank(a.priority) - U.prioRank(b.priority));
  const undated = open.length - dated.length;
  const t0 = U.today();

  const byDay = new Map();
  for (const ev of c.events) for (const d of daysOf(ev)) (byDay.get(d) || byDay.set(d, []).get(d)).push(ev);

  const sections = [];
  sections.push(section('over', t('Overdue'), '', dated.filter((x) => U.daysUntil(x.due) < 0).map(taskRow), true));
  for (let i = 0; i <= 6; i++) {
    const d = U.addDays(t0, i);
    const key = U.ymd(d);
    const rows = [...(byDay.get(key) || []).map((ev) => eventRow(ev)), ...dated.filter((x) => x.due === key).map(taskRow)];
    sections.push(section(key, i === 0 ? t('Today') : i === 1 ? t('Tomorrow') : U.weekday(d), U.shortDate(d), rows));
  }
  const nextWeek = [];
  for (let i = 7; i < AGENDA_DAYS; i++) {
    const key = U.ymd(U.addDays(t0, i));
    for (const ev of byDay.get(key) || []) nextWeek.push(eventRow(ev, key));
  }
  nextWeek.push(...dated.filter((x) => U.daysUntil(x.due) >= 7 && U.daysUntil(x.due) < AGENDA_DAYS).map(taskRow));
  sections.push(section('next', t('Next week'), `${U.shortDate(U.addDays(t0, 7))} – ${U.shortDate(U.addDays(t0, AGENDA_DAYS - 1))}`, nextWeek));
  sections.push(section('later', t('Later'), t('tasks only'), dated.filter((x) => U.daysUntil(x.due) >= AGENDA_DAYS).map(taskRow)));
  const any = sections.some(Boolean);

  return h('div', { class: 'agenda' },
    calendarBar(),
    c.error ? h('div', { class: 'banner err' }, icon('alert', 17), h('span', null, c.error)) : null,
    c.problems.map((p) => h('div', { class: 'banner warn', key: 'p' + p.calendarId }, icon('alert', 17), h('span', null, h('b', null, p.calendar + ': '), U.errorText(p.message), p.showingOldCopy ? ' ' + t('Showing the last copy that was read.') : ''))),
    any ? sections : h('div', { class: 'well' }, empty('calendar', t('Nothing on the agenda'), c.list.length ? t('No meetings in the next two weeks and no task has a due date.') : t('Give a task a due date, or connect your calendar to see your meetings here.'))),
    undated
      ? h('div', { class: 'row', style: { margin: '4px 6px 16px' } }, h('span', { class: 'muted' }, tn(undated, '{n} open task has no due date.', '{n} open tasks have no due date.')),
          h('button', { class: 'btn quiet sm', type: 'button', onclick: () => { Object.assign(state.tasksUi, { mode: 'list', group: 'active', sort: 'due', mine: false, assignee: null, priority: null, search: '' }); go('tasks'); } }, t('Show them'), icon('chevRight', 14)))
      : null,
    c.list.length === 0
      ? h('div', { class: 'note-card' }, icon('calendar', 17),
          h('span', null, rich('**See your meetings here.** Connect a Google Calendar with its private link. It is read-only: Headway can show your events but cannot change them. The link stays on this computer.')))
      : null);
}
