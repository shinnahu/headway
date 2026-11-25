// Branch Explorer: every branch is a horizontal lane, time runs left to right,
// green arrows are merges. Right-click a branch to switch to it.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, loadGraph, gitAction, openMenu, openModal, currentProject, setLocalPath, toast, openTask } from '../store.js';
import { spinner, empty } from '../components.js';
import { openUrl, copyText, pickFolder, HAS_FOLDER_PICKER } from '../api.js';
import * as U from '../util.js';

const COL = 46, ROW = 78, PAD_L = 70, PAD_T = 70, R = 8, BAND = 32;
const view = { x: 0, y: 0, k: 1, forKey: null };
let canvas = null;
let ctx = null;
let observer = null;
let drag = null;
let hover = null;
let setup = { path: '', busy: false, error: null };

const wx = (col) => PAD_L + col * COL;
const wy = (row) => PAD_T + row * ROW;
const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

function palette() {
  return {
    accent: css('--accent'), accentSoft: css('--accent-soft'), accentSofter: css('--accent-softer'), ink: css('--ink'), ink2: css('--ink-2'), ink3: css('--ink-3'),
    ink4: css('--ink-4'), line: css('--line'), lineStrong: css('--line-strong'), well: css('--well'), surface: css('--surface'), green: css('--green'),
    amber: css('--amber'), amberSoft: css('--amber-soft'), slate: css('--slate'), bg: css('--bg'),
  };
}

/** The name to hand to `git switch` for a lane ("origin/x" lanes switch to "x"). */
function switchName(lane) {
  if (lane.kind === 'merged' || lane.kind === 'loose') return null;
  if (!lane.local && lane.remote && lane.name === lane.remote) return lane.name.split('/').slice(1).join('/');
  return lane.name;
}

const laneOf = (g, node) => g.lanes[node.lane];
const pullFor = (lane) => {
  const snap = state.snap;
  if (!snap) return null;
  const name = switchName(lane) || lane.name;
  return snap.pulls.find((p) => (lane.pull && p.number === lane.pull) || p.headRef === name) || null;
};
const tasksFor = (lane) => {
  const snap = state.snap;
  if (!snap) return [];
  const pr = pullFor(lane);
  const numbers = new Set(pr ? pr.tasks : []);
  const lead = /^(\d+)[-_/]/.exec(lane.name);
  if (lead) numbers.add(Number(lead[1]));
  for (const t of snap.tasks) if (t.branches.includes(lane.name)) numbers.add(t.number);
  return snap.tasks.filter((t) => numbers.has(t.number));
};

// ---- drawing -----------------------------------------------------------------------------

function roundRect(c, x, y, w, hgt, r) {
  c.beginPath();
  c.moveTo(x + r, y);
  c.arcTo(x + w, y, x + w, y + hgt, r);
  c.arcTo(x + w, y + hgt, x, y + hgt, r);
  c.arcTo(x, y + hgt, x, y, r);
  c.arcTo(x, y, x + w, y, r);
  c.closePath();
}

function laneStyle(lane, P) {
  if (lane.isCurrent) return { fill: '#e3efff', stroke: P.accent, node: P.accent, text: P.ink, dash: null };
  if (lane.isDefault) return { fill: P.accentSofter, stroke: P.accentSoft, node: P.accent, text: P.ink, dash: null };
  if (lane.kind === 'merged' || lane.kind === 'loose') return { fill: P.bg, stroke: P.lineStrong, node: P.ink4, text: P.ink3, dash: [4, 4] };
  return { fill: P.well, stroke: P.lineStrong, node: P.slate, text: P.ink2, dash: null };
}

function laneLabel(lane) {
  let text = lane.name;
  if (lane.kind === 'merged') text += lane.pull ? `  (merged, PR #${lane.pull})` : '  (merged)';
  else if (lane.kind === 'remote') text += '  (on GitHub only)';
  else if (lane.kind === 'local') text += '  (local only)';
  if (lane.ahead) text += `  ↑${lane.ahead}`;
  if (lane.behind) text += `  ↓${lane.behind}`;
  return text;
}

function draw() {
  const g = state.graph.data;
  if (!canvas || !ctx || !g) return;
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const hgt = canvas.clientHeight;
  if (!w || !hgt) return;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(hgt * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(hgt * dpr);
  }
  const P = palette();
  const sel = state.graph.sel;
  const c = ctx;
  c.setTransform(dpr, 0, 0, dpr, 0, 0);
  c.clearRect(0, 0, w, hgt);
  if (!g.nodes.length) return;

  const k = view.k;
  const first = Math.max(0, Math.floor((-view.x / k - PAD_L) / COL) - 2);
  const last = Math.min(g.nodes.length - 1, Math.ceil(((w - view.x) / k - PAD_L) / COL) + 2);
  const seen = (a, b) => b >= first && a <= last;

  c.save();
  c.translate(view.x, view.y);
  c.scale(k, k);

  // day separators
  c.strokeStyle = P.line;
  c.lineWidth = 1 / k;
  let prevDay = null;
  const dayMarks = [];
  for (let i = Math.max(0, first - 1); i <= last; i++) {
    const d = U.dayOf(g.nodes[i].date);
    if (i > 0 && d !== prevDay && prevDay !== null) {
      const x = wx(i) - COL / 2;
      c.beginPath();
      c.moveTo(x, -view.y / k);
      c.lineTo(x, (hgt - view.y) / k);
      c.stroke();
    }
    if (d !== prevDay) dayMarks.push({ col: i, date: new Date(g.nodes[i].date) });
    prevDay = d;
  }

  // lane bands
  for (const lane of g.lanes) {
    if (!seen(lane.fromCol, lane.toCol)) continue;
    const s = laneStyle(lane, P);
    const y = wy(lane.row);
    const x0 = wx(lane.firstCol) - 18;
    const x1 = wx(lane.lastCol) + 18;
    roundRect(c, x0, y - BAND / 2, x1 - x0, BAND, BAND / 2);
    c.fillStyle = s.fill;
    c.fill();
    c.lineWidth = sel && sel.kind === 'lane' && sel.id === lane.id ? 2.4 : 1.2;
    c.strokeStyle = sel && sel.kind === 'lane' && sel.id === lane.id ? P.ink : s.stroke;
    c.setLineDash(s.dash || []);
    c.stroke();
    c.setLineDash([]);
  }

  // links between lanes (drawn under the nodes)
  for (const link of g.links) {
    if (link.kind === 'flow' || !seen(link.from, link.to)) continue;
    const a = g.nodes[link.from];
    const b = g.nodes[link.to];
    const ax = wx(a.col), ay = wy(g.lanes[a.lane].row), bx = wx(b.col), by = wy(g.lanes[b.lane].row);
    const mid = ax + (bx - ax) * 0.5;
    c.beginPath();
    c.moveTo(ax, ay);
    c.bezierCurveTo(mid, ay, mid, by, bx - (link.kind === 'merge' ? R + 7 : 0), by);
    c.lineWidth = link.kind === 'merge' ? 1.9 : 1.5;
    c.strokeStyle = link.kind === 'merge' ? P.green : P.ink4;
    c.stroke();
    if (link.kind === 'merge') {
      c.beginPath();
      c.moveTo(bx - R - 1, by);
      c.lineTo(bx - R - 9, by - 4.5);
      c.lineTo(bx - R - 9, by + 4.5);
      c.closePath();
      c.fillStyle = P.green;
      c.fill();
    }
  }

  // the line through each lane
  for (const link of g.links) {
    if (link.kind !== 'flow' || !seen(link.from, link.to)) continue;
    const a = g.nodes[link.from];
    const lane = g.lanes[a.lane];
    const y = wy(lane.row);
    c.beginPath();
    c.moveTo(wx(link.from), y);
    c.lineTo(wx(link.to), y);
    c.lineWidth = 2;
    c.strokeStyle = laneStyle(lane, P).node;
    c.stroke();
  }

  // commits
  c.textBaseline = 'middle';
  for (let i = first; i <= last; i++) {
    const n = g.nodes[i];
    const lane = g.lanes[n.lane];
    const s = laneStyle(lane, P);
    const x = wx(n.col), y = wy(lane.row);
    const isHead = n.sha === g.head;
    const picked = sel && sel.kind === 'commit' && sel.sha === n.sha;
    const hot = hover && hover.kind === 'commit' && hover.sha === n.sha;
    if (isHead || picked || hot) {
      c.beginPath();
      c.arc(x, y, R + 5, 0, Math.PI * 2);
      c.fillStyle = picked ? 'rgba(27,27,33,0.14)' : isHead ? 'rgba(10,132,255,0.22)' : 'rgba(27,27,33,0.07)';
      c.fill();
    }
    c.beginPath();
    c.arc(x, y, n.parentCount > 1 ? R - 2 : R, 0, Math.PI * 2);
    c.fillStyle = isHead ? P.accent : n.parentCount > 1 ? s.node : P.surface;
    c.fill();
    c.lineWidth = 2.2;
    c.strokeStyle = isHead ? P.accent : s.node;
    c.stroke();
    if (isHead) {
      c.beginPath();
      c.arc(x, y, 2.6, 0, Math.PI * 2);
      c.fillStyle = '#fff';
      c.fill();
    }
    if (n.hiddenParents) {
      c.beginPath();
      c.setLineDash([2, 3]);
      c.moveTo(x - R - 2, y);
      c.lineTo(x - R - 16, y);
      c.strokeStyle = P.ink4;
      c.lineWidth = 1.5;
      c.stroke();
      c.setLineDash([]);
    }
    // tags and branch names that do not own a lane
    const chips = n.refs.filter((r) => r.kind === 'tag' || (r.name !== lane.name && r.name !== lane.remote && !(isHead && r.name === g.headBranch)));
    let cy = y + BAND / 2 + 11;
    c.font = '600 10.5px ' + css('--font');
    for (const r of chips.slice(0, 3)) {
      const label = r.kind === 'tag' ? r.name : r.name;
      const tw = c.measureText(label).width + 12;
      roundRect(c, x - tw / 2, cy - 8, tw, 16, 5);
      c.fillStyle = r.kind === 'tag' ? P.amberSoft : P.well;
      c.fill();
      c.fillStyle = r.kind === 'tag' ? P.amber : P.ink2;
      c.textAlign = 'center';
      c.fillText(label, x, cy + 0.5);
      cy += 19;
    }
  }

  // lane names, above the start of each band
  c.textAlign = 'left';
  for (const lane of g.lanes) {
    if (!seen(lane.fromCol, lane.toCol)) continue;
    const s = laneStyle(lane, P);
    const y = wy(lane.row) - BAND / 2 - 9;
    let x = wx(lane.firstCol) - 16;
    // Keep the name of a long lane in view while its start is scrolled away.
    const leftEdge = (8 - view.x) / k;
    const maxX = wx(lane.lastCol) - 40;
    if (x < leftEdge && maxX > leftEdge) x = leftEdge;
    c.font = (lane.isCurrent || lane.isDefault ? '700 ' : '600 ') + '12px ' + css('--font');
    c.fillStyle = lane.isCurrent ? P.accent : s.text;
    c.fillText((lane.isCurrent ? '⌂ ' : '') + laneLabel(lane), x, y);
  }
  c.restore();

  // date axis, fixed to the top
  c.fillStyle = 'rgba(255,255,255,0.92)';
  c.fillRect(0, 0, w, 26);
  c.strokeStyle = P.line;
  c.lineWidth = 1;
  c.beginPath();
  c.moveTo(0, 26.5);
  c.lineTo(w, 26.5);
  c.stroke();
  c.font = '500 11px ' + css('--font');
  c.fillStyle = P.ink3;
  c.textAlign = 'left';
  c.textBaseline = 'middle';
  let lastX = -1e9;
  for (const m of dayMarks) {
    const x = (wx(m.col) - COL / 2) * k + view.x + 6;
    if (x - lastX < 62) continue;
    c.fillText(U.shortDate(m.date), Math.max(6, x), 13);
    lastX = Math.max(6, x);
  }
}

// ---- interaction ---------------------------------------------------------------------------

function toWorld(e) {
  const r = canvas.getBoundingClientRect();
  return { x: (e.clientX - r.left - view.x) / view.k, y: (e.clientY - r.top - view.y) / view.k };
}

function hit(e) {
  const g = state.graph.data;
  if (!g) return null;
  const p = toWorld(e);
  const col = Math.round((p.x - PAD_L) / COL);
  for (const i of [col, col - 1, col + 1]) {
    const n = g.nodes[i];
    if (!n) continue;
    const y = wy(g.lanes[n.lane].row);
    if (Math.abs(p.x - wx(n.col)) <= R + 6 && Math.abs(p.y - y) <= R + 6) return { kind: 'commit', sha: n.sha, node: n };
  }
  for (const lane of g.lanes) {
    const y = wy(lane.row);
    const x0 = wx(lane.firstCol) - 18, x1 = wx(lane.lastCol) + 18;
    if (p.x >= x0 && p.x <= x1 && p.y >= y - BAND / 2 - 20 && p.y <= y + BAND / 2) return { kind: 'lane', id: lane.id, lane };
  }
  return null;
}

function bounds() {
  const g = state.graph.data;
  const rows = g ? Math.max(1, ...g.lanes.map((l) => l.row + 1)) : 1;
  return { w: PAD_L + (g ? g.nodes.length : 0) * COL + 40, h: PAD_T + rows * ROW + 20 };
}

function clampView() {
  const b = bounds();
  const w = canvas.clientWidth, hgt = canvas.clientHeight;
  const minX = Math.min(0, w - b.w * view.k);
  const minY = Math.min(0, hgt - b.h * view.k);
  view.x = U.clamp(view.x, minX, 0);
  view.y = U.clamp(view.y, minY, 0);
}

function showLatest() {
  if (!canvas) return;
  const b = bounds();
  view.x = Math.min(0, canvas.clientWidth - b.w * view.k);
  view.y = 0;
  clampView();
  draw();
}

function zoom(factor, cx0, cy0) {
  const k = U.clamp(view.k * factor, 0.45, 2);
  const x = cx0 ?? canvas.clientWidth / 2;
  const y = cy0 ?? canvas.clientHeight / 2;
  view.x = x - ((x - view.x) / view.k) * k;
  view.y = y - ((y - view.y) / view.k) * k;
  view.k = k;
  clampView();
  draw();
}

function branchMenu(lane, at) {
  const g = state.graph.data;
  const repo = state.snap ? state.snap.repo : null;
  const name = switchName(lane);
  const pr = pullFor(lane);
  const tasks = tasksFor(lane);
  const items = [
    { label: lane.isCurrent ? 'You are on this branch' : 'Switch to this branch', sub: name ? null : 'This branch no longer exists', icon: 'home', disabled: lane.isCurrent || !name || !!state.graph.busy, onClick: () => gitAction('switch', 'git_switch', { branch: name }, `Switched to ${name}`) },
    { label: 'New branch from here…', icon: 'branch', onClick: () => openModal({ kind: 'newBranch', from: lane.tip, fromLabel: `the tip of ${lane.name}` }) },
    'sep',
  ];
  if (pr) items.push({ label: `Open pull request #${pr.number}`, sub: pr.title, icon: 'pr', onClick: () => openUrl(pr.url) });
  for (const t of tasks.slice(0, 3)) items.push({ label: `Open task #${t.number}`, sub: t.title, icon: 'tasks', onClick: () => openTask(t.number) });
  if (repo && lane.remote) items.push({ label: 'Open branch on GitHub', icon: 'external', onClick: () => openUrl(`${repo.url}/tree/${(name || lane.name).split('/').map(encodeURIComponent).join('/')}`) });
  items.push({ label: 'Copy branch name', icon: 'copy', onClick: async () => { if (await copyText(name || lane.name)) toast('Branch name copied'); } });
  if (g && lane.isCurrent) items.unshift({ title: 'Current branch' });
  openMenu(at, items, { minWidth: 250 });
}

function commitMenu(node, at) {
  const g = state.graph.data;
  const repo = state.snap ? state.snap.repo : null;
  const lane = laneOf(g, node);
  const name = switchName(lane);
  openMenu(at, [
    { title: node.sha.slice(0, 7) + ' on ' + lane.name },
    { label: 'New branch from this commit…', icon: 'branch', onClick: () => openModal({ kind: 'newBranch', from: node.sha, fromLabel: `commit ${node.sha.slice(0, 7)}` }) },
    { label: `Switch to ${name || lane.name}`, icon: 'home', disabled: lane.isCurrent || !name || !!state.graph.busy, onClick: () => gitAction('switch', 'git_switch', { branch: name }, `Switched to ${name}`) },
    'sep',
    repo ? { label: 'Open commit on GitHub', icon: 'external', onClick: () => openUrl(`${repo.url}/commit/${node.sha}`) } : null,
    { label: 'Copy commit id', icon: 'copy', onClick: async () => { if (await copyText(node.sha)) toast('Commit id copied'); } },
  ].filter(Boolean), { minWidth: 250 });
}

function attach(el) {
  canvas = el;
  ctx = el.getContext('2d');
  if (observer) observer.disconnect();
  observer = new ResizeObserver(() => { clampView(); draw(); });
  observer.observe(el);

  el.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
    el.setPointerCapture(e.pointerId);
  });
  el.addEventListener('pointermove', (e) => {
    if (drag) {
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
      if (drag.moved) {
        view.x = drag.vx + dx;
        view.y = drag.vy + dy;
        clampView();
        el.classList.add('panning');
        el.removeAttribute('data-tip');
        draw();
      }
      return;
    }
    const target = hit(e);
    const key = target ? target.kind + (target.sha || target.id) : '';
    if (key !== (hover ? hover.key : '')) {
      hover = target ? { ...target, key } : null;
      el.classList.toggle('pointing', !!target);
      if (target && target.kind === 'commit') {
        const n = target.node;
        el.setAttribute('data-tip', `${n.subject}\n${n.author} · ${new Date(n.date).toLocaleString()} · ${n.sha.slice(0, 7)}`);
      } else if (target) {
        el.setAttribute('data-tip', `${laneLabel(target.lane)}\n${U.plural(target.lane.commitCount, 'commit')} · right-click for actions`);
      } else {
        el.removeAttribute('data-tip');
      }
      draw();
    }
  });
  const end = (e) => {
    if (!drag) return;
    const wasDrag = drag.moved;
    drag = null;
    el.classList.remove('panning');
    if (!wasDrag && e.type === 'pointerup') {
      const target = hit(e);
      state.graph.sel = target ? (target.kind === 'commit' ? { kind: 'commit', sha: target.sha } : { kind: 'lane', id: target.id }) : null;
      render();
    }
  };
  el.addEventListener('pointerup', end);
  el.addEventListener('pointercancel', end);
  el.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    const target = hit(e);
    if (!target) return;
    state.graph.sel = target.kind === 'commit' ? { kind: 'commit', sha: target.sha } : { kind: 'lane', id: target.id };
    if (target.kind === 'commit') commitMenu(target.node, { x: e.clientX, y: e.clientY });
    else branchMenu(target.lane, { x: e.clientX, y: e.clientY });
  });
  el.addEventListener('wheel', (e) => {
    e.preventDefault();
    if (e.ctrlKey) {
      const r = el.getBoundingClientRect();
      zoom(e.deltaY < 0 ? 1.12 : 1 / 1.12, e.clientX - r.left, e.clientY - r.top);
      return;
    }
    const tall = bounds().h * view.k > el.clientHeight;
    if (tall && !e.shiftKey) view.y -= e.deltaY;
    else view.x -= e.deltaY + e.deltaX;
    if (tall) view.x -= e.shiftKey ? 0 : e.deltaX;
    clampView();
    draw();
  }, { passive: false });
  el.addEventListener('pointerleave', () => {
    if (hover) {
      hover = null;
      el.removeAttribute('data-tip');
      draw();
    }
  });
}

/** Called after every draw of the app: keeps the canvas in step with the data. */
export function afterBranchesDraw() {
  const el = document.getElementById('bx-canvas');
  if (!el) {
    canvas = null;
    ctx = null;
    if (observer) observer.disconnect();
    observer = null;
    return;
  }
  if (el !== canvas) attach(el);
  const g = state.graph.data;
  if (!g) return;
  const key = `${state.graph.forProject}:${g.nodes.length}:${g.nodes.length ? g.nodes[g.nodes.length - 1].sha : ''}`;
  if (view.forKey !== key) {
    view.forKey = key;
    view.k = 1;
    showLatest();
  } else {
    draw();
  }
}

// ---- the DOM around the canvas -----------------------------------------------------------

function kv(rows) {
  return h('dl', { class: 'kv' }, rows.filter(Boolean).map(([k, v]) => [h('dt', null, k), h('dd', null, v)]));
}

function copyable(text, short) {
  return h('span', { class: 'row', style: { gap: '4px' } }, h('code', { 'data-tip': text }, short || text),
    h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': 'Copy', onclick: async () => { if (await copyText(text)) toast('Copied'); } }, icon('copy', 13)));
}

function laneProps(g, lane) {
  const repo = state.snap ? state.snap.repo : null;
  const pr = pullFor(lane);
  const tasks = tasksFor(lane);
  const tip = g.nodes.find((n) => n.sha === lane.tip);
  const name = switchName(lane);
  const where = lane.kind === 'both' ? 'On this computer and on GitHub' : lane.kind === 'local' ? 'On this computer only (not pushed)' : lane.kind === 'remote' ? 'On GitHub only (not checked out here)' : lane.kind === 'merged' ? `Merged into ${lane.mergedInto || 'another branch'} and deleted` : 'No branch points here';
  const comments = pr ? pr.title : tasks.length ? tasks[0].title : tip ? tip.subject : '';
  return [
    h('div', { class: 'panel-head' }, icon('branch', 17), h('span', { class: 'truncate' }, lane.name)),
    h('div', { class: 'scroll' },
      h('div', { class: 'row', style: { margin: '12px 0', 'flex-wrap': 'wrap' } },
        h('button', { class: 'btn primary sm', type: 'button', disabled: lane.isCurrent || !name || !!state.graph.busy, onclick: () => gitAction('switch', 'git_switch', { branch: name }, `Switched to ${name}`) }, state.graph.busy === 'switch' ? spinner() : icon('home', 14), lane.isCurrent ? 'Current branch' : 'Switch to this branch'),
        h('button', { class: 'btn sm', type: 'button', onclick: () => openModal({ kind: 'newBranch', from: lane.tip, fromLabel: `the tip of ${lane.name}` }) }, icon('branch', 14), 'Branch from here')),
      kv([
        ['Name', lane.name],
        ['Repository', repo ? repo.nameWithOwner : state.projectId],
        ['Creation date', h('span', { 'data-tip': 'Git does not record when a branch was created: this is the date of its first own commit.' }, new Date(lane.createdAt).toLocaleString())],
        ['Owner', h('span', { 'data-tip': 'Author of the branch\'s first own commit' }, lane.owner)],
        ['Guid', h('span', { 'data-tip': 'Git branches have no id of their own: this is the head commit.' }, copyable(lane.tip, lane.tip.slice(0, 12)))],
        ['Comments', comments || h('span', { class: 'muted' }, 'None')],
        ['Where', where],
        ['Commits', `${lane.commitCount} of its own` + (lane.ahead || lane.behind ? ` · ${lane.ahead} to push, ${lane.behind} to pull` : '')],
        lane.mergedInto && lane.kind !== 'merged' ? ['Merged', `into ${lane.mergedInto}`] : null,
        pr ? ['Pull request', h('button', { class: 'pr-chip ' + U.pullKind(pr), type: 'button', onclick: () => openUrl(pr.url) }, icon(pr.state === 'merged' ? 'merge' : 'pr', 13), `#${pr.number} ${U.PULL_LABEL[U.pullKind(pr)]}`)] : null,
        tasks.length ? ['Tasks', h('span', { class: 'row', style: { 'flex-wrap': 'wrap', gap: '6px' } }, tasks.map((t) => h('button', { class: 'pr-chip', type: 'button', 'data-tip': t.title, onclick: () => openTask(t.number) }, icon('tasks', 13), '#' + t.number)))] : null,
      ])),
  ];
}

function commitProps(g, node) {
  const repo = state.snap ? state.snap.repo : null;
  const lane = laneOf(g, node);
  return [
    h('div', { class: 'panel-head' }, icon('commit', 17), h('span', { class: 'truncate' }, node.sha.slice(0, 7))),
    h('div', { class: 'scroll' },
      h('div', { class: 'row', style: { margin: '12px 0', 'flex-wrap': 'wrap' } },
        h('button', { class: 'btn sm', type: 'button', onclick: () => openModal({ kind: 'newBranch', from: node.sha, fromLabel: `commit ${node.sha.slice(0, 7)}` }) }, icon('branch', 14), 'Branch from here'),
        repo ? h('button', { class: 'btn sm', type: 'button', onclick: () => openUrl(`${repo.url}/commit/${node.sha}`) }, icon('external', 14), 'GitHub') : null),
      kv([
        ['Comments', node.subject],
        ['Branch', h('button', { class: 'btn quiet sm', type: 'button', style: { 'margin-left': '-10px' }, onclick: () => { state.graph.sel = { kind: 'lane', id: lane.id }; render(); } }, lane.name)],
        ['Repository', repo ? repo.nameWithOwner : state.projectId],
        ['Creation date', new Date(node.date).toLocaleString()],
        ['Owner', `${node.author}`],
        ['Guid', copyable(node.sha, node.sha.slice(0, 12))],
        ['Kind', node.parentCount > 1 ? 'Merge commit' : node.parentCount === 0 ? 'First commit' : 'Commit'],
        node.refs.length ? ['Labels', node.refs.map((r) => r.name).join(', ')] : null,
        node.sha === g.head ? ['Working copy', 'Your files are at this commit'] : null,
      ])),
  ];
}

function setupCard(project) {
  const save = async (path) => {
    setup.busy = true;
    setup.error = null;
    render();
    try {
      await setLocalPath(project.id, path);
    } catch (e) {
      setup.error = U.errorText(e);
    }
    setup.busy = false;
    render();
  };
  return h('div', { class: 'setup' },
    h('div', { class: 'panel' },
      h('span', { style: { color: 'var(--accent)', display: 'inline-block' } }, icon('branches', 34)),
      h('h2', null, 'Show Headway your local clone'),
      h('p', null, `The Branch Explorer reads the history from the folder where you cloned ${project.id}, and switches branches there. Nothing is uploaded.`),
      HAS_FOLDER_PICKER
        ? h('div', { class: 'row' }, h('button', { class: 'btn primary', type: 'button', disabled: setup.busy, onclick: async () => { const p = await pickFolder(); if (p) save(p); } }, setup.busy ? spinner() : icon('folder', 16), 'Choose the folder…'))
        : h('form', { class: 'row', onsubmit: (e) => { e.preventDefault(); if (setup.path.trim()) save(setup.path.trim()); } },
            h('input', { class: 'input', style: { width: '320px' }, placeholder: 'C:\\Work\\my-repo', spellcheck: 'false', value: setup.path, oninput: (e) => { setup.path = e.target.value; } }),
            h('button', { class: 'btn primary', type: 'submit', disabled: setup.busy }, setup.busy ? spinner() : null, 'Use this folder')),
      setup.error ? h('p', { class: 'error-text', style: { 'margin-top': '12px' } }, setup.error) : null));
}

export function branchesView() {
  const project = currentProject();
  if (!project) return h('div', null);
  if (!project.localPath) return setupCard(project);
  const gs = state.graph;
  const g = gs.data;
  if (!g) {
    if (gs.error) return h('div', { class: 'well' }, empty('alert', 'Could not read the local repository', gs.error, h('button', { class: 'btn', type: 'button', style: { 'margin-top': '10px' }, onclick: loadGraph }, icon('refresh', 16), 'Try again')));
    return h('div', { class: 'loading', style: { height: '50vh' } }, h('div', { class: 'row' }, spinner(), 'Reading the repository…'));
  }
  const tree = g.tree;
  const changed = tree.staged + tree.unstaged + tree.conflicted;
  const dirty = changed + tree.untracked > 0;
  const sel = gs.sel;
  const selLane = sel && sel.kind === 'lane' ? g.lanes[sel.id] : null;
  const selNode = sel && sel.kind === 'commit' ? g.nodes.find((n) => n.sha === sel.sha) : null;
  const current = g.lanes.find((l) => l.isCurrent) || null;
  const shownLane = selLane || (!selNode ? current || g.lanes[0] : null);

  return h('div', null,
    h('div', { class: 'toolbar' },
      h('span', { class: 'branch-pill', 'data-tip': 'The branch your local files are on' }, icon('home', 15), g.detached ? `Detached at ${(g.head || '').slice(0, 7)}` : g.headBranch || 'No branch'),
      h('span', { class: cx('tree-state', dirty && 'dirty'), 'data-tip': dirty ? 'Uncommitted work. Git will refuse a switch that would overwrite it.' : 'No uncommitted changes' },
        dirty ? `${changed ? U.plural(changed, 'changed file') : ''}${changed && tree.untracked ? ', ' : ''}${tree.untracked ? U.plural(tree.untracked, 'new file') : ''}` : 'Working copy clean'),
      tree.ahead || tree.behind ? h('span', { class: 'tree-state' }, `${tree.ahead} to push · ${tree.behind} to pull`) : null,
      h('span', { class: 'spacer' }),
      g.truncated ? h('button', { class: 'btn sm', type: 'button', onclick: () => { gs.limit = Math.min(5000, gs.limit * 2); loadGraph(); } }, icon('history', 14), 'Load older history') : null,
      h('button', { class: 'btn sm', type: 'button', disabled: !!gs.busy, 'data-tip': 'Download the latest branches and commits from GitHub', onclick: () => gitAction('fetch', 'git_fetch', {}, 'Fetched from GitHub') }, gs.busy === 'fetch' ? spinner() : icon('download', 14), 'Fetch'),
      h('button', { class: 'icon-btn', type: 'button', 'aria-label': 'Reload', 'data-tip': 'Re-read the local repository', onclick: loadGraph }, gs.loading ? spinner() : icon('refresh', 16))),
    h('div', { class: 'bx' },
      h('div', { class: 'bx-canvas' },
        h('div', { keep: true, style: { position: 'absolute', inset: '0' } }, h('canvas', { id: 'bx-canvas', role: 'img', 'aria-label': `Branch graph of ${project.id}: ${g.lanes.length} branches, ${g.nodes.length} commits` })),
        h('div', { class: 'bx-hud' },
          h('button', { class: 'icon-btn', type: 'button', 'aria-label': 'Zoom out', onclick: () => zoom(1 / 1.2) }, icon('zoomOut', 16)),
          h('button', { class: 'icon-btn', type: 'button', 'aria-label': 'Zoom in', onclick: () => zoom(1.2) }, icon('zoomIn', 16)),
          h('button', { class: 'btn sm', type: 'button', onclick: () => { view.k = 1; showLatest(); } }, icon('target', 14), 'Latest')),
        h('div', { class: 'bx-note' }, g.nodes.length ? 'Drag to move · Ctrl + wheel to zoom · right-click a branch' : 'This repository has no commits yet')),
      h('aside', { class: 'bx-side' }, selNode ? commitProps(g, selNode) : shownLane ? laneProps(g, shownLane) : h('div', { class: 'empty' }, 'Select a branch or a commit'))));
}
