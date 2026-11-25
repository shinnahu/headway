// Demo backend, used when the address has ?mock. Everything here is invented
// sample data (a fictional team and repository); nothing is sent anywhere.

const NOW = Date.now();
const pad = (n) => String(n).padStart(2, '0');
const ymd = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
/** ISO time `daysAgo` days back at the given local hour (never in the future). */
const at = (daysAgo, hour = 10, min = 0) => {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, min, 0, 0);
  if (d.getTime() > NOW - 60000) d.setTime(NOW - (10 + min) * 60000);
  return d.toISOString();
};
/** YYYY-MM-DD `offset` days from today. */
const day = (offset) => {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return ymd(d);
};
const wait = (ms = 120) => new Promise((r) => setTimeout(r, ms));
const clone = (v) => JSON.parse(JSON.stringify(v));

const GH = 'https://github.com/northbay/harbor';
const person = (login, name) => ({ login, name, avatarUrl: '', url: 'https://github.com/' + login });
const maya = person('maya-ortiz', 'Maya Ortiz');
const leo = person('leo-brandt', 'Leo Brandt');
const ines = person('ines-duarte', 'Ines Duarte');
const tom = person('tom-reyes', 'Tom Reyes');

const members = [
  { ...maya, role: 'admin' },
  { ...ines, role: 'write' },
  { ...leo, role: 'write' },
  { ...tom, role: 'write' },
];

const pullRef = (number, title, state, isDraft = false) => ({ number, title, url: `${GH}/pull/${number}`, state, isDraft });

let nextNumber = 56;
function task(number, title, status, o = {}) {
  return {
    id: 'I_' + number, number, title, status,
    body: o.body || '', url: `${GH}/issues/${number}`,
    priority: o.priority || null, due: o.due || null,
    assignees: o.assignees || [], author: o.author || maya, labels: o.labels || [],
    commentCount: o.comments || 0,
    createdAt: at(o.created ?? 14, 9), updatedAt: at(o.updated ?? 1, 15),
    closedAt: o.closed != null ? at(o.closed, o.closedHour ?? 16) : null,
    pulls: o.pulls || [], branches: o.branches || [],
  };
}

const tasks = [
  task(38, 'Refund flow for cancelled sailings', 'in_progress', { priority: 'urgent', due: day(0), assignees: [maya], created: 12, updated: 0, comments: 6, pulls: [pullRef(51, 'Refund flow for cancelled sailings', 'open', true)], body: 'When a sailing is cancelled every booking on it should be refunded without the customer asking.\n\n- [x] Refund request form\n- [x] Rules per fare class\n- [ ] Refund emails\n- [ ] Report for the finance export' }),
  task(41, 'Seat map on the booking screen', 'in_review', { priority: 'high', due: day(1), assignees: [leo], created: 9, updated: 0, comments: 4, pulls: [pullRef(52, 'Seat map on the booking screen', 'open')] }),
  task(48, 'Crew app: scan QR tickets', 'in_progress', { priority: 'medium', due: day(4), assignees: [ines, tom], created: 2, updated: 0, comments: 2, pulls: [pullRef(53, 'Scan QR tickets in the crew app', 'open')] }),
  task(44, 'Offline boarding list for crew tablets', 'in_progress', { priority: 'high', due: day(3), assignees: [ines], created: 8, updated: 1, comments: 3, branches: ['44-offline-boarding-list'] }),
  task(33, 'Weekly sales report email', 'in_review', { priority: 'low', due: day(-1), assignees: [maya], created: 20, updated: 1, comments: 5, pulls: [pullRef(50, 'Weekly sales report email', 'open')] }),
  task(36, 'Timetable import breaks on the DST change', 'todo', { priority: 'urgent', due: day(-2), assignees: [leo], created: 16, updated: 2, comments: 7, labels: [{ name: 'bug', color: 'd73a4a' }] }),
  task(45, 'Translate booking emails to Spanish', 'todo', { priority: 'medium', due: day(6), assignees: [tom], created: 7, updated: 3, comments: 1 }),
  task(47, 'Price rules for vehicles over 5 m', 'todo', { priority: 'medium', due: day(9), created: 4, updated: 4 }),
  task(49, 'Audit log for fare changes', 'todo', { priority: 'low', created: 2, updated: 2 }),
  task(55, 'Document the release checklist', 'todo', { priority: 'low', due: day(12), assignees: [maya], created: 0, updated: 0, labels: [{ name: 'documentation', color: '0075ca' }] }),
  task(54, 'Fare rounding is off by a cent on return trips', 'done', { priority: 'high', assignees: [tom], created: 10, closed: 0, closedHour: 9, updated: 0, comments: 3, pulls: [pullRef(46, 'Fix fare rounding for return trips', 'merged')], labels: [{ name: 'bug', color: 'd73a4a' }] }),
  task(43, 'Boarding pass as PDF', 'done', { priority: 'medium', assignees: [ines], created: 15, closed: 1, updated: 1, comments: 2, pulls: [pullRef(43, 'Boarding pass PDF', 'merged')] }),
  task(37, 'Timetable queries are slow on the search page', 'done', { priority: 'medium', assignees: [leo], created: 18, closed: 1, closedHour: 11, updated: 1, comments: 4, pulls: [pullRef(42, 'Cache timetable queries', 'merged')] }),
  task(35, 'Crew roster screen', 'done', { priority: 'medium', assignees: [ines], created: 22, closed: 2, updated: 2, comments: 8, pulls: [pullRef(40, 'Crew roster screen', 'merged')] }),
  task(34, 'Run the tests on every pull request', 'done', { priority: 'high', assignees: [maya], created: 24, closed: 4, updated: 4, comments: 1 }),
  task(32, 'Passenger count limit per sailing', 'done', { priority: 'high', assignees: [leo], created: 25, closed: 5, updated: 5, comments: 2 }),
  task(31, 'Export bookings as CSV', 'done', { priority: 'low', assignees: [tom], created: 28, closed: 8, updated: 8 }),
  task(30, 'Fare table and price rules', 'done', { priority: 'high', assignees: [maya], created: 30, closed: 11, updated: 11, comments: 5 }),
  task(29, 'Try websockets for seat locks', 'canceled', { assignees: [leo], created: 15, closed: 3, updated: 3, comments: 2 }),
];

const checks = (passed, total, failed = 0) => ({ state: failed ? 'failure' : passed === total ? 'success' : 'pending', total, passed, failed, pending: total - passed - failed });
function pull(number, title, state, author, o = {}) {
  return {
    id: 'PR_' + number, number, title, url: `${GH}/pull/${number}`, state, isDraft: !!o.draft, author,
    headRef: o.head || `${number}-branch`, baseRef: 'main', headOid: 'f'.repeat(40),
    createdAt: at(o.created ?? 3, 10), updatedAt: at(o.updated ?? 0, 14),
    mergedAt: o.merged != null ? at(o.merged, o.mergedHour ?? 17) : null, closedAt: o.merged != null ? at(o.merged, o.mergedHour ?? 17) : o.closed != null ? at(o.closed, 12) : null,
    additions: o.add ?? 120, deletions: o.del ?? 30, changedFiles: o.files ?? 6,
    review: o.review || null, checks: o.checks || null, tasks: o.tasks || [], comments: o.comments || 0, reviewers: o.reviewers || [], mergeCommit: o.merged != null ? 'c'.repeat(40) : null,
  };
}

const pulls = [
  pull(53, 'Scan QR tickets in the crew app', 'open', ines, { head: '48-scan-qr', created: 1, updated: 0, checks: checks(17, 18), review: 'review_required', tasks: [48], reviewers: [tom], add: 412, del: 37, files: 14 }),
  pull(52, 'Seat map on the booking screen', 'open', leo, { head: '41-seat-map', created: 4, updated: 0, checks: checks(18, 18), review: 'approved', tasks: [41], add: 688, del: 92, files: 21 }),
  pull(51, 'Refund flow for cancelled sailings', 'open', maya, { head: '38-refund-flow', draft: true, created: 3, updated: 0, checks: checks(15, 18, 1), tasks: [38], add: 530, del: 64, files: 17 }),
  pull(50, 'Weekly sales report email', 'open', maya, { head: '33-sales-report', created: 6, updated: 1, checks: checks(16, 18), review: 'changes_requested', tasks: [33], add: 240, del: 18, files: 9 }),
  pull(46, 'Fix fare rounding for return trips', 'merged', tom, { head: 'fix/fare-rounding', created: 2, merged: 0, mergedHour: 9, updated: 0, checks: checks(18, 18), review: 'approved', tasks: [54], add: 48, del: 12, files: 3 }),
  pull(43, 'Boarding pass PDF', 'merged', ines, { head: '43-boarding-pass', created: 3, merged: 1, updated: 1, checks: checks(18, 18), review: 'approved', tasks: [43], add: 356, del: 20, files: 11 }),
  pull(42, 'Cache timetable queries', 'merged', leo, { head: '37-cache-timetable', created: 2, merged: 1, mergedHour: 11, updated: 1, checks: checks(18, 18), review: 'approved', tasks: [37], add: 190, del: 75, files: 8 }),
  pull(40, 'Crew roster screen', 'merged', ines, { head: '40-crew-roster', created: 5, merged: 2, updated: 2, checks: checks(17, 17), review: 'approved', tasks: [35], add: 820, del: 110, files: 26 }),
  pull(39, 'Experiment: websockets for seat locks', 'closed', leo, { head: 'experiment/websockets', created: 9, closed: 3, updated: 3, checks: checks(12, 17, 2), tasks: [], add: 300, del: 5, files: 7 }),
];

// Commits per day for the last 14 days, oldest first (the last entry is today).
const PER_DAY = [3, 1, 2, 5, 2, 0, 1, 6, 3, 8, 0, 7, 2, 5];
const committers = [leo, ines, maya, tom];
const recentCommits = [];
PER_DAY.forEach((count, i) => {
  const daysAgo = PER_DAY.length - 1 - i;
  for (let k = 0; k < count; k++) {
    const who = committers[(i + k) % committers.length];
    recentCommits.push({ committedAt: at(daysAgo, 9 + ((k * 2) % 9), (k * 13) % 60), authorLogin: who.login, authorName: who.name });
  }
});

const repo = {
  id: 'R_demo', nameWithOwner: 'northbay/harbor', name: 'harbor', ownerLogin: 'northbay', ownerAvatar: '',
  description: 'Booking and boarding for small ferry lines.', url: GH, homepageUrl: null,
  isPrivate: true, isArchived: false, hasIssues: true, createdAt: at(420), pushedAt: at(0, 9),
  defaultBranch: 'main',
  lastCommit: { oid: 'a41c9e0f7b2d4c66a1f0e3b59d7c8a2b4e6f1d03', headline: 'Fix timezone in timetable import', committedAt: new Date(NOW - 2 * 3600000).toISOString(), authorName: leo.name, authorLogin: leo.login, authorAvatar: '', url: `${GH}/commit/a41c9e0` },
  totalCommits: 1284, branchCount: 7, openIssues: 10, closedIssues: 29, openPulls: 4, mergedPulls: 37, closedPulls: 2,
  stars: 0, forks: 0, primaryLanguage: { name: 'Rust', color: '#dea584' }, viewerPermission: 'ADMIN',
  recentCommits, recentCommitsTruncated: false,
};

const projects = [
  { id: 'northbay/harbor', owner: 'northbay', repo: 'harbor', localPath: 'C:\\Work\\harbor', addedAt: at(30) },
  { id: 'northbay/harbor-crew-app', owner: 'northbay', repo: 'harbor-crew-app', addedAt: at(12) },
];
let currentId = projects[0].id;
const settings = { syncSeconds: 30, githubClientId: null };

const snapshot = () => clone({ projectId: currentId, fetchedAt: new Date().toISOString(), viewer: maya, repo, tasks, pulls, members, labels: [{ name: 'bug', color: 'd73a4a' }, { name: 'documentation', color: '0075ca' }], tasksTruncated: false, fromCache: false });

const escapeHtml = (s) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
function renderBody(md) {
  if (!md) return '';
  const lines = md.split('\n');
  let html = '';
  let list = false;
  for (const line of lines) {
    const item = /^- \[( |x)\] (.*)$/.exec(line);
    if (item) {
      if (!list) html += '<ul class="contains-task-list">';
      list = true;
      html += `<li class="task-list-item"><input type="checkbox" disabled${item[1] === 'x' ? ' checked' : ''}> ${escapeHtml(item[2])}</li>`;
      continue;
    }
    if (list) {
      html += '</ul>';
      list = false;
    }
    if (line.trim()) html += `<p>${escapeHtml(line)}</p>`;
  }
  if (list) html += '</ul>';
  return html;
}

const sampleComments = (t) => {
  const n = Math.min(t.commentCount, 3);
  const lines = [
    [leo, 'I can take the review on this one. Is the fare table change already on main?'],
    [maya, 'Yes, merged yesterday. The **refund rules** read from it now.'],
    [ines, 'Tested on the crew tablet build: works offline, syncs when the ferry is back in range.'],
  ];
  return lines.slice(0, n).map(([who, body], i) => ({
    id: `C_${t.number}_${i}`, author: who, body, bodyHtml: `<p>${escapeHtml(body).replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')}</p>`,
    createdAt: at(Math.max(0, 2 - i), 10 + i), updatedAt: at(Math.max(0, 2 - i), 10 + i), url: `${t.url}#issuecomment-${i}`,
    canEdit: who.login === maya.login, canDelete: true, isMine: who.login === maya.login,
  }));
};
const detail = (t) => clone({ ...t, bodyHtml: renderBody(t.body), comments: t._comments || (t._comments = sampleComments(t)) });
const find = (number) => {
  const t = tasks.find((x) => x.number === Number(number));
  if (!t) throw { code: 'not_found', message: `There is no task #${number} in ${currentId}.` };
  return t;
};
const users = (logins) => (logins || []).map((l) => members.find((m) => m.login === l)).filter(Boolean).map((m) => person(m.login, m.name));

// ---- a small hand-built branch graph ---------------------------------------------

function graph() {
  const sha = (i) => (i * 2654435761 >>> 0).toString(16).padStart(8, '0').repeat(5);
  const C = [
    [0, 0, leo, 'Initial timetable model'], [1, 0, maya, 'Booking API skeleton'], [2, 0, maya, 'Fare table and price rules'],
    [3, 1, tom, 'Round fares per leg'], [4, 1, tom, 'Add rounding tests'], [5, 0, maya, 'CI: run tests on pull requests'],
    [6, 1, tom, 'Handle return trips'], [7, 2, ines, 'Roster model'], [8, 0, maya, 'Merge pull request #46 from northbay/fix/fare-rounding'],
    [9, 0, leo, 'Cache timetable queries'], [10, 2, ines, 'Roster screen'], [11, 2, ines, 'Shift swap'],
    [12, 0, ines, 'Boarding pass PDF'], [13, 3, leo, 'Seat map component'], [14, 0, maya, 'Merge pull request #40 from northbay/40-crew-roster'],
    [15, 3, leo, 'Seat selection state'], [16, 4, maya, 'Refund request form'], [17, 0, tom, 'Translate booking emails (scaffold)'],
    [18, 3, leo, 'Seat map on the booking screen'], [19, 4, maya, 'Refund rules per fare class'], [20, 5, ines, 'QR scanner view'],
    [21, 0, leo, 'Show seat availability in search'], [22, 4, maya, 'Refund emails'], [23, 5, ines, 'Validate ticket signature'],
    [24, 3, leo, 'Polish the seat map on small screens'], [25, 0, leo, 'Fix timezone in timetable import'],
  ];
  const refs = {
    12: [{ name: 'v0.9.0', kind: 'tag' }],
    22: [{ name: '38-refund-flow', kind: 'local' }],
    23: [{ name: 'origin/48-scan-qr', kind: 'remote' }],
    24: [{ name: '41-seat-map', kind: 'local' }, { name: 'origin/41-seat-map', kind: 'remote' }],
    25: [{ name: 'main', kind: 'local' }, { name: 'origin/main', kind: 'remote' }],
  };
  const merges = { 8: 6, 14: 11 };
  const nodes = C.map(([col, lane, who, subject]) => ({
    sha: sha(col + 1), author: who.name, email: `${who.login}@example.com`, date: at(Math.round((25 - col) * 0.55), 9 + (col % 8), (col * 7) % 60),
    subject, lane, col, refs: refs[col] || [], parentCount: merges[col] != null ? 2 : col === 0 ? 0 : 1, hiddenParents: false,
  }));
  const laneDefs = [
    { id: 0, name: 'main', row: 0, kind: 'both', local: true, remote: 'origin/main', isDefault: true },
    { id: 1, name: 'fix/fare-rounding', row: 1, kind: 'merged', base: 2, mergedInto: 'main', pull: 46 },
    { id: 2, name: '40-crew-roster', row: 2, kind: 'merged', base: 5, mergedInto: 'main', pull: 40 },
    { id: 3, name: '41-seat-map', row: 1, kind: 'both', local: true, remote: 'origin/41-seat-map', base: 12 },
    { id: 4, name: '38-refund-flow', row: 2, kind: 'local', local: true, base: 14, isCurrent: true, ahead: 3 },
    { id: 5, name: '48-scan-qr', row: 3, kind: 'remote', remote: 'origin/48-scan-qr', base: 17 },
  ];
  const links = [];
  const lanes = laneDefs.map((l) => {
    const cols = nodes.filter((n) => n.lane === l.id).map((n) => n.col);
    for (let i = 1; i < cols.length; i++) links.push({ from: cols[i - 1], to: cols[i], kind: 'flow' });
    if (l.base != null) links.push({ from: l.base, to: cols[0], kind: 'fork' });
    const first = cols[0];
    const last = cols[cols.length - 1];
    const mergeTo = Object.entries(merges).find(([, from]) => from === last);
    return {
      id: l.id, name: l.name, row: l.row, firstCol: first, lastCol: last, fromCol: l.base != null ? l.base : first, toCol: mergeTo ? Number(mergeTo[0]) : last,
      tip: nodes[last].sha, base: l.base != null ? nodes[l.base].sha : null, kind: l.kind, local: !!l.local, remote: l.remote || null,
      ahead: l.ahead || 0, behind: 0, isDefault: !!l.isDefault, isCurrent: l.name === mockHead,
      createdAt: nodes[first].date, owner: nodes[first].author, commitCount: cols.length, mergedInto: l.mergedInto || null, pull: l.pull || null,
    };
  });
  for (const [to, from] of Object.entries(merges)) links.push({ from, to: Number(to), kind: 'merge' });
  const branches = lanes.filter((l) => l.kind !== 'merged').map((l) => ({ name: l.name, local: l.local, remote: l.remote, tip: l.tip, lane: l.id, ahead: l.ahead, behind: 0, isDefault: l.isDefault, isCurrent: l.isCurrent }));
  const headLane = lanes.find((l) => l.name === mockHead) || lanes[0];
  return {
    root: 'C:\\Work\\harbor', nodes, lanes, links, branches, head: headLane.tip, headBranch: mockHead, detached: false, defaultBranch: 'main', truncated: false,
    tree: { head: headLane.tip, branch: mockHead, detached: false, upstream: null, ahead: headLane.ahead, behind: 0, staged: 0, unstaged: 2, untracked: 1, conflicted: 0 },
  };
}
let mockHead = '38-refund-flow';

// ---- a sample calendar ---------------------------------------------------------------

let demoCalendars = [{ id: 'demo', name: 'Team calendar', addedAt: at(20) }];

/** Meetings for the next two weeks, relative to today. */
function demoEvents() {
  if (!demoCalendars.length) return [];
  const out = [];
  const local = (offset, hour, min) => {
    const d = new Date();
    d.setDate(d.getDate() + offset);
    d.setHours(hour, min, 0, 0);
    return d;
  };
  const timed = (id, title, offset, h1, m1, minutes, extra = {}) => {
    const start = local(offset, h1, m1);
    out.push({ id: `${id}@${offset}`, calendarId: 'demo', calendar: 'Team calendar', title, allDay: false, start: start.toISOString(), end: new Date(start.getTime() + minutes * 60000).toISOString(), location: null, link: null, recurring: false, ...extra });
  };
  for (let i = 0; i < 14; i++) {
    const weekday = local(i, 12, 0).getDay();
    if (weekday >= 1 && weekday <= 5) timed('standup', 'Daily standup', i, 9, 30, 15, { recurring: true, link: 'https://meet.google.com/' });
    if (weekday === 5) timed('review', 'Sprint review and demo', i, 16, 0, 60, { recurring: true, location: 'Harbour room', link: 'https://meet.google.com/' });
    if (weekday === 1) timed('planning', 'Sprint planning', i, 11, 0, 45, { recurring: true });
  }
  timed('design', 'Seat map design sync', 1, 14, 0, 30, { link: 'https://meet.google.com/' });
  timed('ops', 'Call with the ferry operators', 2, 15, 30, 45, { location: 'Dock office' });
  out.push({ id: 'freeze@3', calendarId: 'demo', calendar: 'Team calendar', title: 'Release freeze', allDay: true, start: day(3), end: day(5), location: null, link: null, recurring: false });
  return out.sort((a, b) => a.start.slice(0, 10).localeCompare(b.start.slice(0, 10)) || Number(b.allDay) - Number(a.allDay) || a.start.localeCompare(b.start));
}

// ---- the fake API --------------------------------------------------------------------

export async function call(method, p = {}) {
  await wait(method === 'snapshot' ? 180 : 90);
  switch (method) {
    case 'app_info':
      return { name: 'Headway', version: 'demo', configPath: '(demo data, nothing is saved)', mcpPath: 'C:\\Program Files\\Headway\\headway-mcp.exe' };
    case 'auth_status':
      return { signedIn: true, user: maya, login: maya.login, source: 'gh', ghAccounts: [{ login: maya.login, active: true }], deviceFlowConfigured: false };
    case 'settings_get':
      return clone(settings);
    case 'settings_set':
      Object.assign(settings, p);
      return clone(settings);
    case 'projects_list':
      return { projects: clone(projects), currentId };
    case 'projects_select':
      currentId = p.id;
      return { projects: clone(projects), currentId };
    case 'projects_add':
    case 'projects_remove':
    case 'projects_set_path':
      throw { code: 'invalid', message: 'This is the demo: projects cannot be changed here.' };
    case 'repos_list':
      return [
        { nameWithOwner: 'northbay/harbor', description: repo.description, isPrivate: true, isArchived: false, pushedAt: at(0), viewerPermission: 'ADMIN', ownerAvatar: '' },
        { nameWithOwner: 'northbay/harbor-crew-app', description: 'Tablet app for the crew.', isPrivate: true, isArchived: false, pushedAt: at(1), viewerPermission: 'WRITE', ownerAvatar: '' },
        { nameWithOwner: 'northbay/timetables', description: 'Timetable data and import scripts.', isPrivate: false, isArchived: false, pushedAt: at(9), viewerPermission: 'WRITE', ownerAvatar: '' },
      ];
    case 'snapshot':
      return snapshot();
    case 'snapshot_cached':
      return null;
    case 'sync':
      return { changed: false };
    case 'task_get':
      return detail(find(p.number));
    case 'task_create': {
      const t = task(nextNumber++, p.title, p.status || 'todo', { body: p.body, priority: p.priority, due: p.due, assignees: users(p.assignees), created: 0, updated: 0 });
      tasks.unshift(t);
      repo.openIssues += 1;
      return detail(t);
    }
    case 'task_update': {
      const t = find(p.number);
      const wasOpen = ['todo', 'in_progress', 'in_review'].includes(t.status);
      if (p.title) t.title = p.title;
      if (p.body != null) t.body = p.body;
      if (p.status) t.status = p.status;
      if ('priority' in p) t.priority = p.priority || null;
      if ('due' in p) t.due = p.due || null;
      if (p.assignees) t.assignees = users(p.assignees);
      const isOpen = ['todo', 'in_progress', 'in_review'].includes(t.status);
      if (wasOpen !== isOpen) {
        t.closedAt = isOpen ? null : new Date().toISOString();
        repo.openIssues += isOpen ? 1 : -1;
        repo.closedIssues += isOpen ? -1 : 1;
      }
      t.updatedAt = new Date().toISOString();
      return detail(t);
    }
    case 'comment_add': {
      const t = find(p.number);
      const c = { id: `C_${t.number}_${Date.now()}`, author: maya, body: p.body, bodyHtml: `<p>${escapeHtml(p.body)}</p>`, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), url: t.url, canEdit: true, canDelete: true, isMine: true };
      detail(t);
      t._comments.push(c);
      t.commentCount += 1;
      return clone(c);
    }
    case 'comment_delete':
      for (const t of tasks) if (t._comments) t._comments = t._comments.filter((c) => c.id !== p.id);
      return { deleted: true };
    case 'task_start_branch': {
      const t = find(p.number);
      const branch = p.name || `${t.number}-${t.title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 40)}`;
      t.branches = [branch];
      return { branch, base: 'main', url: `${GH}/tree/${branch}` };
    }
    case 'pull_link':
    case 'pull_unlink':
      return { linked: method === 'pull_link', changed: true, note: null };
    case 'git_graph':
      return graph();
    case 'git_status':
      return graph().tree;
    case 'git_switch':
      mockHead = p.branch;
      return graph().tree;
    case 'git_fetch':
      return { fetched: true };
    case 'git_create_branch':
      return graph().tree;
    case 'local_inspect':
      return { root: p.path, remotes: [{ name: 'origin', url: GH + '.git', repo: 'northbay/harbor' }], branch: 'main' };
    case 'open_url':
      window.open(p.url, '_blank', 'noopener');
      return { opened: true };
    case 'calendar_list':
      return clone(demoCalendars);
    case 'calendar_events':
      return { calendars: clone(demoCalendars), events: demoEvents(), problems: [] };
    case 'calendar_add':
      throw { code: 'invalid', message: 'This is the demo: it already has a sample calendar and reads no real one.' };
    case 'calendar_remove':
      demoCalendars = demoCalendars.filter((c) => c.id !== p.id);
      return clone(demoCalendars);
    case 'auth_sign_out':
      throw { code: 'invalid', message: 'This is the demo: there is nothing to sign out of.' };
    default:
      throw { code: 'invalid', message: `The demo does not implement ${method}.` };
  }
}
