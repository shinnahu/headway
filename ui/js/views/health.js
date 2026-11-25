// Project health: the repository at a glance (the screen modelled on style.png).
import { h } from '../dom.js';
import { icon } from '../icons.js';
import { state, openTask } from '../store.js';
import { ring, meter, barChart, lineChart } from '../charts.js';
import { avatar, pullPill, empty } from '../components.js';
import { openUrl } from '../api.js';
import { t, tn, N } from '../i18n.js';
import * as U from '../util.js';

const sum =(list) => list.reduce((a, b) => a + b, 0);
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
  const dated = tasks.filter((x) => U.isOpen(x) && x.due);
  const overdue = dated.filter((x) => U.daysUntil(x.due) < 0);

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
  const doneBetween = (from, to) => tasks.filter((x) => x.status === 'done' && x.closedAt && new Date(x.closedAt) >= from && new Date(x.closedAt) < to).length;
  const doneThisWeek = doneBetween(weekStart, U.addDays(t0, 1));
  const doneLastWeek = doneBetween(lastWeekStart, weekStart);

  // Open tasks at the end of each of the last 7 days.
  const openAt = (when) => tasks.filter((x) => new Date(x.createdAt) <= when && (!x.closedAt || new Date(x.closedAt) > when)).length;
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

const VS_LAST_WEEK = { more: N('more than last week'), fewer: N('fewer than last week'), same: N('same as last week') };
const VS_WEEK_AGO = { more: N('more than a week ago'), fewer: N('fewer than a week ago'), same: N('same as a week ago') };

/** The change as an arrow and a number, then in words; `extra` is appended. */
function delta(n, words, extra) {
  const tail = extra ? ' · ' + extra : '';
  if (n > 0) return [h('span', { class: 'delta' }, icon('arrowUp', 12), U.fmt(n)), h('small', null, t(words.more) + tail)];
  if (n < 0) return [h('span', { class: 'delta down' }, icon('arrowDown', 12), U.fmt(-n)), h('small', null, t(words.fewer) + tail)];
  return [h('span', { class: 'delta flat' }, icon('minus', 12)), h('small', null, t(words.same) + tail)];
}

function tile(iconName, label, big, small, ringValue, tip) {
  return h('div', { class: 'tile', 'data-tip': tip || null },
    h('div', { class: 'tile-label' }, icon(iconName, 17), label),
    h('div', { class: 'tile-row' },
      h('div', { class: 'figure' }, h('b', null, big), small ? h('span', null, small) : null),
      ringValue == null ? null : ring(ringValue)));
}

function mergeTime(hours) {
  if (hours == null) return ['–', t('no merges yet')];
  if (hours < 1) return [String(Math.max(1, Math.round(hours * 60))), t('min median')];
  if (hours < 24) return [hours < 10 ? U.fmt(Math.round(hours * 10) / 10) : String(Math.round(hours)), t('hours median')];
  const d = hours / 24;
  return [d < 10 ? U.fmt(Math.round(d * 10) / 10) : String(Math.round(d)), t('days median')];
}

function repoStrip(repo) {
  const c = repo.lastCommit;
  return h('div', { class: 'repo-strip' },
    h('a', { class: 'name', href: repo.url, onclick: (e) => { e.preventDefault(); openUrl(repo.url); } }, icon('repo', 16), repo.nameWithOwner),
    h('span', { class: 'tag' }, repo.isPrivate ? t('Private') : t('Public')),
    repo.description ? h('span', { class: 'about truncate', 'data-tip': repo.description }, repo.description) : h('span', { class: 'about muted' }, t('No description on GitHub yet')),
    c
      ? h('span', { class: 'last' },
          h('span', { class: 'muted', style: { 'white-space': 'nowrap' } }, t('Last commit')),
          h('b', { class: 'truncate', 'data-tip': c.headline }, c.headline),
          h('span', { class: 'sep' }, '·'),
          h('span', { style: { 'white-space': 'nowrap' }, 'data-tip': U.dateTime(c.committedAt) }, U.ago(c.committedAt)),
          avatar({ login: c.authorLogin || c.authorName, name: c.authorName, avatarUrl: c.authorAvatar || '' }, 20))
      : h('span', { class: 'last muted' }, t('No commits yet')));
}

function reviewIcon(p) {
  const r = p.review;
  if (r === 'approved') return h('span', { class: 'review-ic approved', 'data-tip': t('Approved') }, icon('checkCircle', 19));
  if (r === 'changes_requested') return h('span', { class: 'review-ic changes_requested', 'data-tip': t('Changes requested') }, icon('alert', 19));
  if (r === 'review_required') return h('span', { class: 'review-ic', 'data-tip': t('Waiting for review') }, icon('eye', 19));
  return h('span', { class: 'review-ic', 'data-tip': t('No review requested') }, icon('minus', 19));
}

function checksCell(p) {
  const c = p.checks;
  if (!c || !c.total) return h('span', { class: 'muted' }, t('No checks'));
  const bad = c.failed > 0;
  const tip = t('{passed} passed, {failed} failed, {pending} running', { passed: c.passed, failed: c.failed, pending: c.pending });
  return h('div', { class: 'checks', 'data-tip': tip },
    h('span', { class: 'bar' }, h('i', { class: bad ? 'bad' : null, style: { width: U.pct(c.passed, c.total) + '%' } })),
    h('b', { class: bad ? 'bad' : null }, `${c.passed}/${c.total}`));
}

function pullRow(p) {
  return h('tr', { class: 'click', key: 'pr' + p.number, onclick: () => openUrl(p.url) },
    h('td', null, h('div', { class: 'title-cell' }, h('span', { class: 'bullet' }), h('span', { class: 'truncate' }, p.title), h('span', { class: 'num' }, '#' + p.number),
      p.tasks.map((n) => h('button', { class: 'pr-chip', type: 'button', style: { 'margin-left': '8px' }, 'data-tip': t('Closes task #{n}', { n }), onclick: (e) => { e.stopPropagation(); openTask(n); } }, icon('tasks', 13), '#' + n)))),
    h('td', { class: 'num', style: { 'white-space': 'nowrap' }, 'data-tip': U.dateTime(p.updatedAt) }, U.shortDate(new Date(p.updatedAt))),
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
  const range = `${U.shortDate(first)} – ${U.shortDate(last)}`;
  const openNow = m.openTrend[m.openTrend.length - 1].value;
  const openThen = m.openTrend[0].value;
  const shown = pulls.slice(0, 12);
  const branch = repo.defaultBranch || t('the default branch');

  return h('div', { class: 'health' },
    repoStrip(repo),

    h('div', { class: 'tiles' },
      tile('checkCircle', t('Tasks done'), m.totalTasks ? m.donePct + '%' : '–', m.totalTasks ? `${U.fmt(repo.closedIssues)} / ${U.fmt(m.totalTasks)}` : t('no tasks yet'), m.totalTasks ? m.donePct : null, t('Closed tasks out of all tasks')),
      tile('merge', t('Pull requests merged'), m.totalPulls ? m.mergedPct + '%' : '–', m.totalPulls ? `${U.fmt(repo.mergedPulls)} / ${U.fmt(m.totalPulls)}` : t('none yet'), m.totalPulls ? m.mergedPct : null, t('Merged pull requests out of all pull requests')),
      tile('clock', t('On schedule'), m.onTimePct == null ? '–' : m.onTimePct + '%', m.onTimePct == null ? t('no due dates') : `${m.dated - m.overdue} / ${m.dated}`, m.onTimePct, t('Open tasks with a due date that are not overdue')),
      tile('history', t('Time to merge'), mergeBig, mergeSmall, null, t('Median time from opening a pull request to merging it'))),

    h('div', { class: 'health-grid two' },
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('commit', 17), t('Commits on {branch}', { branch })),
        h('div', { class: 'card stat' },
          h('div', { class: 'stat-top' }, h('b', null, U.fmt(repo.totalCommits)), delta(m.commitsThisWeek - m.commitsLastWeek, VS_LAST_WEEK, tn(m.commitsThisWeek, '{n} commit in the last 7 days', '{n} commits in the last 7 days'))),
          h('div', { class: 'dash' }),
          meter(U.pct(m.activeDays, 7), t('{n} of the last 7 days had commits', { n: m.activeDays })),
          h('div', { class: 'stat-foot' }, h('span', null, tn(7 - m.activeDays, '{n} quiet day this week', '{n} quiet days this week')), h('b', null, t('{n} of 7 days active', { n: m.activeDays }))))),
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('checkCircle', 17), t('Tasks completed')),
        h('div', { class: 'card stat' },
          h('div', { class: 'stat-top' }, h('b', null, U.fmt(repo.closedIssues)), delta(m.doneThisWeek - m.doneLastWeek, VS_LAST_WEEK, t('{n} done in the last 7 days', { n: U.fmt(m.doneThisWeek) }))),
          h('div', { class: 'dash' }),
          meter(m.donePct, t('{n} percent of tasks are done', { n: m.donePct })),
          h('div', { class: 'stat-foot' }, h('span', null, t('{n} still open', { n: U.fmt(repo.openIssues) })), h('b', null, t('{n}% done', { n: m.donePct })))))),

    h('div', { class: 'health-grid charts', style: { 'margin-top': '14px' } },
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('pulse', 17), t('Commit activity'),
          h('div', { class: 'right' },
            h('span', { class: 'legend' }, h('span', null, h('i'), t('This week')), h('span', null, h('i', { class: 'light' }), t('Last week'))),
            h('span', { class: 'range-pill' }, icon('calendar', 14), range))),
        h('div', { class: 'card chart' },
          barChart(m.commits.map((d) => ({ label: String(d.date.getDate()), name: U.shortDate(d.date), value: d.value, light: d.light, tip: `${U.weekday(d.date)}, ${U.shortDate(d.date)}\n${U.nCommits(d.value)}` })), { caption: t('Commits per day on {branch}, last 14 days', { branch }) }))),
      h('div', { class: 'well' },
        h('div', { class: 'well-head' }, icon('trend', 17), t('Open tasks')),
        h('div', { class: 'card chart' },
          h('div', { class: 'stat-top', style: { padding: '4px 2px 6px' } }, h('b', null, U.fmt(openNow)), delta(openNow - openThen, VS_WEEK_AGO)),
          lineChart(m.openTrend.map((d) => ({ label: String(d.date.getDate()), name: U.shortDate(d.date), value: d.value, tip: `${U.weekday(d.date)}, ${U.shortDate(d.date)}\n${tn(d.value, '{n} open task', '{n} open tasks')}` })), { height: 150, caption: t('Open tasks at the end of each day, last 7 days') })))),

    h('div', { class: 'section-title' }, icon('pr', 17), t('Pull requests'),
      h('span', { class: 'right muted' }, t('{open} open · {merged} merged', { open: U.fmt(repo.openPulls), merged: U.fmt(repo.mergedPulls) }))),
    shown.length
      ? h('table', { class: 'table' },
          h('thead', null, h('tr', null, [t('Pull request'), t('Updated'), t('Author'), t('Checks'), t('Review'), t('Status')].map((c) => h('th', null, c)))),
          h('tbody', null, shown.map(pullRow)))
      : h('div', { class: 'well' }, empty('pr', t('No pull requests yet'), t('When someone opens one on GitHub it shows up here.'))));
}
