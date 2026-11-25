// Project health: the repository at a glance (the screen modelled on style.png).
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, openTask } from '../store.js';
import { ring, meter, barChart, lineChart } from '../charts.js';
import { avatar, pullPill, empty } from '../components.js';
import { openUrl } from '../api.js';
import * as U from '../util.js';

const sum = (list) => list.reduce((a, b) => a + b, 0);
const median = (list) => {
  if (!list.length) return null;
  const s = [...list].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
};

/** Everything the screen shows, computed from the snapshot. */
export function metrics(snap) {
  const { repo, tasks, pulls } = snap;
  const t0 = U.today();
  const endOf = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate(), 23, 59, 59, 999);

  const totalTasks = repo.openIssues + repo.closedIssues;
  const totalPulls = repo.openPulls + repo.mergedPulls + repo.closedPulls;
  const dated = tasks.filter((t) => U.isOpen(t) && t.due);
  const overdue = dated.filter((t) => U.daysUntil(t.due) < 0);

  // Commits per local day, last 14 days; the earlier 7 are the comparison week.
  const days = [];
  for (let i = 13; i >= 0; i--) days.push(U.addDays(t0, -i));
  const perDay = new Map(days.map((d) => [U.ymd(d), 0]));
  for (const c of repo.recentCommits || []) {
    const k = U.dayOf(c.committedAt);
    if (perDay.has(k)) perDay.set(k, perDay.get(k) + 1);
  }
  const commits = days.map((d, i) => ({ date: d, value: perDay.get(U.ymd(d)), light: i < 7 }));
  const commitsThisWeek = sum(commits.slice(7).map((d) => d.value));
  const commitsLastWeek = sum(commits.slice(0, 7).map((d) => d.value));
  const activeDays = commits.slice(7).filter((d) => d.value > 0).length;

  const weekStart = U.addDays(t0, -6);
  const lastWeekStart = U.addDays(t0, -13);
  const doneBetween = (from, to) => tasks.filter((t) => t.status === 'done' && t.closedAt && new Date(t.closedAt) >= from && new Date(t.closedAt) < to).length;
  const doneThisWeek = doneBetween(weekStart, U.addDays(t0, 1));
  const doneLastWeek = doneBetween(lastWeekStart, weekStart);

  // Open tasks at the end of each of the last 7 days.
  const openAt = (when) => tasks.filter((t) => new Date(t.createdAt) <= when && (!t.closedAt || new Date(t.closedAt) > when)).length;
  const openTrend = [];
  for (let i = 6; i >= 0; i--) {
    const d = U.addDays(t0, -i);
    openTrend.push({ date: d, value: i === 0 ? tasks.filter(U.isOpen).length : openAt(endOf(d)) });
  }

  const mergeHours = pulls.filter((p) => p.state === 'merged' && p.mergedAt).map((p) => (new Date(p.mergedAt) - new Date(p.createdAt)) / 3600000);

  return {
    totalTasks, totalPulls, dated: dated.length, overdue: overdue.length,
    donePct: U.pct(repo.closedIssues, totalTasks),
    mergedPct: U.pct(repo.mergedPulls, totalPulls),
    onTimePct: dated.length ? U.pct(dated.length - overdue.length, dated.length) : null,
    mergeMedianHours: median(mergeHours),
    commits, commitsThisWeek, commitsLastWeek, activeDays,
    doneThisWeek, doneLastWeek, openTrend,
  };
}

function delta(n, word) {
  if (n > 0) return h('span', { class: 'delta' }, icon('arrowUp', 12), `${U.fmt(n)} ${word}`);
  if (n < 0) return h('span', { class: 'delta down' }, icon('arrowDown', 12), `${U.fmt(-n)} ${word}`);
  return h('span', { class: 'delta flat' }, icon('minus', 12), 'no change');
}

function tile(iconName, label, big, small, ringValue, tip) {
  return h('div', { class: 'tile', 'data-tip': tip || null },
    h('div', { class: 'tile-label' }, icon(iconName, 17), label),
    h('div', { class: 'tile-row' },
      h('div', { class: 'figure' }, h('b', null, big), small ? h('span', null, small) : null),
      ringValue == null ? null : ring(ringValue)));
}

function mergeTime(hours) {
  if (hours == null) return ['–', 'no merges yet'];
  if (hours < 1) return [String(Math.max(1, Math.round(hours * 60))), 'min median'];
  if (hours < 36) return [hours < 10 ? hours.toFixed(1) : String(Math.round(hours)), 'hours median'];
  const d = hours / 24;
  return [d < 10 ? d.toFixed(1) : String(Math.round(d)), 'days median'];
}

function repoStrip(repo) {
  const c = repo.lastCommit;
  return h('div', { class: 'repo-strip' },
    h('a', { class: 'name', href: repo.url, onclick: (e) => { e.preventDefault(); openUrl(repo.url); } }, icon('repo', 16), repo.nameWithOwner),
    h('span', { class: 'tag' }, repo.isPrivate ? 'Private' : 'Public'),
    repo.description ? h('span', { class: 'about truncate', 'data-tip': repo.description }, repo.description) : h('span', { class: 'about muted' }, 'No description on GitHub yet'),
    c
      ? h('span', { class: 'last' },
          h('span', { class: 'muted', style: { 'white-space': 'nowrap' } }, 'Last commit'),
          h('b', { class: 'truncate', 'data-tip': c.headline }, c.headline),
          h('span', { class: 'sep' }, '·'),
          h('span', { style: { 'white-space': 'nowrap' }, 'data-tip': new Date(c.committedAt).toLocaleString() }, U.ago(c.committedAt)),
          avatar({ login: c.authorLogin || c.authorName, name: c.authorName, avatarUrl: c.authorAvatar || '' }, 20))
      : h('span', { class: 'last muted' }, 'No commits yet'));
}

function reviewIcon(p) {
  const r = p.review;
  if (r === 'approved') return h('span', { class: 'review-ic approved', 'data-tip': 'Approved' }, icon('checkCircle', 19));
  if (r === 'changes_requested') return h('span', { class: 'review-ic changes_requested', 'data-tip': 'Changes requested' }, icon('alert', 19));
  if (r === 'review_required') return h('span', { class: 'review-ic', 'data-tip': 'Waiting for review' }, icon('eye', 19));
  return h('span', { class: 'review-ic', 'data-tip': 'No review requested' }, icon('minus', 19));
}

function checksCell(p) {
  const c = p.checks;
  if (!c || !c.total) return h('span', { class: 'muted' }, 'No checks');
  const bad = c.failed > 0;
  const tip = `${c.passed} passed, ${c.failed} failed, ${c.pending} running`;
  return h('div', { class: 'checks', 'data-tip': tip },
    h('span', { class: 'bar' }, h('i', { class: bad ? 'bad' : null, style: { width: U.pct(c.passed, c.total) + '%' } })),
    h('b', { class: bad ? 'bad' : null }, `${c.passed}/${c.total}`));
}

function pullRow(p) {
  return h('tr', { class: 'click', key: 'pr' + p.number, onclick: () => openUrl(p.url), 'data-tip': null },
    h('td', null, h('div', { class: 'title-cell' }, h('span', { class: 'bullet' }), h('span', { class: 'truncate' }, p.title), h('span', { class: 'num' }, '#' + p.number),
      p.tasks.map((n) => h('button', { class: 'pr-chip', type: 'button', style: { 'margin-left': '8px' }, 'data-tip': `Closes task #${n}`, onclick: (e) => { e.stopPropagation(); openTask(n); } }, icon('tasks', 13), '#' + n)))),
    h('td', { class: 'num', style: { 'white-space': 'nowrap' }, 'data-tip': new Date(p.updatedAt).toLocaleString() }, U.shortDate(new Date(p.updatedAt))),
    h('td', null, h('div', { class: 'who' }, avatar(p.author, 28), h('span', { class: 'truncate' }, U.displayName(p.author)))),
    h('td', null, checksCell(p)),
    h('td', null, reviewIcon(p)),
    h('td', null, pullPill(p)));
}

export function healthView() {
  const snap = state.snap;
  const { repo, pulls } = snap;
  const m = metrics(snap);
  const [mergeBig, mergeSmall] = mergeTime(m.mergeMedianHours);
  const first = m.commits[0].date;
  const last = m.commits[m.commits.length - 1].date;
  const range = first.getMonth() === last.getMonth() ? `${U.shortDate(first)} – ${last.getDate()}` : `${U.shortDate(first)} – ${U.shortDate(last)}`;
  const openNow = m.openTrend[m.openTrend.length - 1].value;
  const openThen = m.openTrend[0].value;
  const shown = pulls.slice(0, 12);

  return h('div', { class: 'health' },
    repoStrip(repo),

    h('div', { class: 'tiles' },
      tile('checkCircle', 'Tasks done', m.totalTasks ? m.donePct + '%' : '–', m.totalTasks ? `${U.fmt(repo.closedIssues)} / ${U.fmt(m.totalTasks)}` : 'no tasks yet', m.totalTasks ? m.donePct : null, 'Closed tasks out of all tasks'),
      tile('merge', 'Pull requests merged', m.totalPulls ? m.mergedPct + '%' : '–', m.totalPulls ? `${U.fmt(repo.mergedPulls)} / ${U.fmt(m.totalPulls)}` : 'none yet', m.totalPulls ? m.mergedPct : null, 'Merged pull requests out of all pull requests'),
      tile('clock', 'On schedule', m.onTimePct == null ? '–' : m.onTimePct + '%', m.onTimePct == null ? 'no due dates' : `${m.dated - m.overdue} / ${m.dated}`, m.onTimePct, 'Open tasks with a due date that are not overdue'),
      tile('history', 'Time to merge', mergeBig, mergeSmall, null, 'Median time from opening a pull request to merging it')),

    h('div', { class: 'health-grid two' },
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('commit', 17), `Commits on ${repo.defaultBranch || 'the default branch'}`),
        h('div', { class: 'card stat' },
          h('div', { class: 'stat-top' }, h('b', null, U.fmt(repo.totalCommits)), delta(m.commitsThisWeek - m.commitsLastWeek, 'vs last week'), h('small', null, `${U.plural(m.commitsThisWeek, 'commit')} in the last 7 days`)),
          h('div', { class: 'dash' }),
          meter(U.pct(m.activeDays, 7), `${m.activeDays} of the last 7 days had commits`),
          h('div', { class: 'stat-foot' }, h('span', null, `${U.plural(7 - m.activeDays, 'quiet day')} this week`), h('b', null, `${m.activeDays} of 7 days active`)))),
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('checkCircle', 17), 'Tasks completed'),
        h('div', { class: 'card stat' },
          h('div', { class: 'stat-top' }, h('b', null, U.fmt(repo.closedIssues)), delta(m.doneThisWeek - m.doneLastWeek, 'vs last week'), h('small', null, `${U.fmt(m.doneThisWeek)} done in the last 7 days`)),
          h('div', { class: 'dash' }),
          meter(m.donePct, `${m.donePct} percent of tasks are done`),
          h('div', { class: 'stat-foot' }, h('span', null, `${U.fmt(repo.openIssues)} still open`), h('b', null, `${m.donePct}% done`))))),

    h('div', { class: 'health-grid charts', style: { 'margin-top': '14px' } },
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('pulse', 17), 'Commit activity',
          h('div', { class: 'right' },
            h('span', { class: 'legend' }, h('span', null, h('i'), 'This week'), h('span', null, h('i', { class: 'light' }), 'Last week')),
            h('span', { class: 'range-pill' }, icon('calendar', 14), range))),
        h('div', { class: 'card chart' },
          barChart(m.commits.map((d) => ({ label: String(d.date.getDate()), name: U.shortDate(d.date), value: d.value, light: d.light, tip: `${U.weekday(d.date)}, ${U.shortDate(d.date)}\n${U.plural(d.value, 'commit')}` })), { caption: `Commits per day on ${repo.defaultBranch || 'the default branch'}, last 14 days` }))),
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('trend', 17), 'Open tasks'),
        h('div', { class: 'card chart' },
          h('div', { class: 'stat-top', style: { padding: '4px 2px 6px' } }, h('b', null, U.fmt(openNow)), delta(openNow - openThen, 'vs a week ago')),
          lineChart(m.openTrend.map((d) => ({ label: String(d.date.getDate()), name: U.shortDate(d.date), value: d.value, tip: `${U.weekday(d.date)}, ${U.shortDate(d.date)}\n${U.plural(d.value, 'open task')}` })), { height: 150, caption: 'Open tasks at the end of each day, last 7 days' })))),

    h('div', { class: 'section-title' }, icon('pr', 17), 'Pull requests',
      h('span', { class: 'right muted' }, `${U.fmt(repo.openPulls)} open · ${U.fmt(repo.mergedPulls)} merged`)),
    shown.length
      ? h('table', { class: 'table' },
          h('thead', null, h('tr', null, ['Pull request', 'Updated', 'Author', 'Checks', 'Review', 'Status'].map((c) => h('th', null, c)))),
          h('tbody', null, shown.map(pullRow)))
      : h('div', { class: 'well' }, empty('pr', 'No pull requests yet', 'When someone opens one on GitHub it shows up here.')));
}
