// App state and the actions that change it. Views read `state` and call these.
import { api } from './api.js';
import { errorText } from './util.js';
import { t, setLanguage } from './i18n.js';

export const VIEWS = ['overview', 'tasks', 'agenda', 'health', 'branches', 'team', 'settings'];

export const state = {
  phase: 'boot', // boot | signin | project | ready
  auth: null,
  user: null,
  projects: [],
  projectId: null,
  settings: { syncSeconds: 30, githubClientId: null, language: null },
  info: null,
  snap: null,
  loadError: null,
  syncing: false,
  syncError: null,
  syncedAt: 0,
  route: { view: 'health', task: null },
  collapsed: false,
  tasksUi: { mode: 'list', group: 'active', mine: false, priority: null, assignee: null, search: '', sort: 'smart' },
  menu: null,
  modal: null,
  toasts: [],
  drawer: null,
  graph: { data: null, loading: false, error: null, sel: null, limit: 400, busy: null, forProject: null },
  gate: { busy: null, error: null, device: null, token: '' },
  // Calendars connected by their private link, and their events for the next two weeks.
  cal: { list: [], events: [], problems: [], loading: false, loadedAt: 0, error: null },
};

/** How many days ahead the Agenda shows meetings for. */
export const AGENDA_DAYS = 14;

let renderer = null;
let queued = false;
export function setRenderer(fn) {
  renderer = fn;
}
/** Re-draws the app on the next microtask (many changes, one draw). */
export function render() {
  if (queued) return;
  queued = true;
  queueMicrotask(() => {
    queued = false;
    if (renderer) renderer();
  });
}
export function set(patch) {
  Object.assign(state, patch);
  render();
}

// ---- toasts, menus, modals ------------------------------------------------

let toastId = 0;
export function toast(text, kind = 'ok') {
  const id = ++toastId;
  state.toasts.push({ id, text, kind });
  render();
  setTimeout(() => {
    state.toasts = state.toasts.filter((x) => x.id !== id);
    render();
  }, kind === 'err' ? 7000 : 3500);
}

/**
 * Opens a menu under `anchor` (an element, or {x, y} for a context menu).
 * items: { label, icon?, sub?, checked?, danger?, disabled?, onClick } | 'sep' | { title }
 */
export function openMenu(anchor, items, opts = {}) {
  let x, y, w = 0;
  if (anchor instanceof Element) {
    const r = anchor.getBoundingClientRect();
    x = opts.alignRight ? r.right : r.left;
    y = r.bottom + 6;
    w = r.width;
  } else {
    x = anchor.x;
    y = anchor.y;
  }
  state.menu = { x, y, items, alignRight: !!opts.alignRight, minWidth: Math.max(opts.minWidth || 0, opts.matchWidth ? w : 0), search: opts.search ? '' : null, placeholder: opts.placeholder || null };
  render();
}
export function closeMenu() {
  if (state.menu) set({ menu: null });
}
export function openModal(modal) {
  set({ modal, menu: null });
}
export function closeModal() {
  if (state.modal) set({ modal: null });
}

// ---- routing --------------------------------------------------------------

export function parseHash() {
  const parts = location.hash.replace(/^#\/?/, '').split('/').filter(Boolean);
  const view = VIEWS.includes(parts[0]) ? parts[0] : 'health';
  const task = parts[1] && /^\d+$/.test(parts[1]) ? Number(parts[1]) : null;
  return { view, task };
}
export function go(view, task = null) {
  const hash = '#/' + view + (task ? '/' + task : '');
  if (location.hash === hash) applyRoute();
  else location.hash = hash;
}
export const openTask = (number) => go(state.route.view, number);
export const closeTask = () => go(state.route.view);

export function applyRoute() {
  const route = parseHash();
  state.route = route;
  state.menu = null;
  if (route.task && state.phase === 'ready') {
    if (!state.drawer || state.drawer.number !== route.task) loadDrawer(route.task);
  } else {
    state.drawer = null;
  }
  if (route.view === 'branches' && state.phase === 'ready') ensureGraph();
  if ((route.view === 'agenda' || route.view === 'settings') && state.phase === 'ready' && Date.now() - state.cal.loadedAt > 5 * 60000) loadAgenda();
  render();
}

// ---- calendars -------------------------------------------------------------------

/** Loads the connected calendars' events from today for the next two weeks. */
export async function loadAgenda(force = false) {
  const c = state.cal;
  if (c.loading) return;
  const now = new Date();
  const from = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const to = new Date(now.getFullYear(), now.getMonth(), now.getDate() + AGENDA_DAYS);
  c.loading = true;
  render();
  try {
    const r = await api('calendar_events', { from: from.toISOString(), to: to.toISOString(), force });
    c.list = r.calendars;
    c.events = r.events;
    c.problems = r.problems;
    c.loadedAt = Date.now();
    c.error = null;
  } catch (e) {
    c.error = errorText(e);
  }
  c.loading = false;
  render();
}

export async function addCalendar(url, name) {
  const r = await api('calendar_add', { url, name: name || null });
  await loadAgenda(true);
  return r;
}

export async function removeCalendar(id) {
  try {
    await api('calendar_remove', { id });
    toast(t('Calendar disconnected'));
  } catch (e) {
    toast(errorText(e), 'err');
  }
  await loadAgenda(true);
}

// ---- start-up, sign-in, projects --------------------------------------------

/** ?lang=es in the address overrides the saved language (for previews and screenshots). */
const forcedLanguage = new URLSearchParams(location.search).get('lang');

function applyLanguage() {
  setLanguage(forcedLanguage || state.settings.language || null);
}

export async function boot() {
  try {
    // The language first, so even the sign-in screen is in the right one.
    try {
      state.settings = await api('settings_get');
    } catch {
      /* keep the defaults */
    }
    applyLanguage();
    render();
    const [auth, info] = await Promise.all([api('auth_status'), api('app_info').catch(() => null)]);
    state.auth = auth;
    state.info = info;
    if (!auth.signedIn) return set({ phase: 'signin' });
    state.user = auth.user;
    await loadProjects();
  } catch (e) {
    state.gate.error = errorText(e);
    set({ phase: 'signin' });
  }
}

export async function loadProjects() {
  const [list, settings] = await Promise.all([api('projects_list'), api('settings_get')]);
  state.projects = list.projects;
  state.projectId = list.currentId;
  state.settings = settings;
  applyLanguage();
  if (!state.projectId) {
    stopSync();
    return set({ phase: 'project', snap: null });
  }
  set({ phase: 'ready' });
  applyRoute();
  await loadSnapshot();
  startSync();
}

async function signedIn(result) {
  state.user = result.user;
  state.auth = { ...(state.auth || {}), signedIn: true, user: result.user };
  state.gate = { busy: null, error: null, device: null, token: '' };
  await loadProjects();
}

export async function signInWithGh(login) {
  state.gate = { ...state.gate, busy: 'gh:' + login, error: null };
  render();
  try {
    await signedIn(await api('auth_use_gh', { login }));
  } catch (e) {
    state.gate = { ...state.gate, busy: null, error: errorText(e) };
    render();
  }
}

export async function signInWithToken(token) {
  state.gate = { ...state.gate, busy: 'token', error: null };
  render();
  try {
    await signedIn(await api('auth_set_token', { token }));
  } catch (e) {
    state.gate = { ...state.gate, busy: null, error: errorText(e) };
    render();
  }
}

let deviceTimer = null;
export async function signInWithDevice() {
  state.gate = { ...state.gate, busy: 'device', error: null, device: null };
  render();
  try {
    const d = await api('auth_device_start');
    state.gate = { ...state.gate, busy: null, device: d };
    render();
    const poll = async () => {
      if (!state.gate.device) return;
      try {
        const r = await api('auth_device_poll');
        if (r.status === 'ok') return signedIn(r);
        if (r.status === 'pending') {
          deviceTimer = setTimeout(poll, (r.interval || d.interval || 5) * 1000);
          return;
        }
        state.gate = { ...state.gate, device: null, error: r.status === 'denied' ? t('The sign-in was cancelled on GitHub.') : t('The code expired. Start again.') };
      } catch (e) {
        state.gate = { ...state.gate, device: null, error: errorText(e) };
      }
      render();
    };
    deviceTimer = setTimeout(poll, (d.interval || 5) * 1000);
  } catch (e) {
    state.gate = { ...state.gate, busy: null, error: errorText(e) };
    render();
  }
}
export function cancelDevice() {
  clearTimeout(deviceTimer);
  state.gate = { ...state.gate, device: null };
  render();
}

export async function signOut() {
  stopSync();
  try {
    await api('auth_sign_out');
  } catch (e) {
    toast(errorText(e), 'err');
  }
  state.user = null;
  state.snap = null;
  state.drawer = null;
  state.modal = null;
  state.menu = null;
  try {
    state.auth = await api('auth_status');
  } catch {
    state.auth = { signedIn: false, ghAccounts: [] };
  }
  set({ phase: 'signin' });
}

export async function selectProject(id) {
  if (id === state.projectId) return;
  stopSync();
  state.projectId = id;
  state.snap = null;
  state.drawer = null;
  state.graph = { data: null, loading: false, error: null, sel: null, limit: 400, busy: null, forProject: null };
  state.menu = null;
  render();
  try {
    await api('projects_select', { id });
  } catch (e) {
    toast(errorText(e), 'err');
  }
  go(state.route.view);
  await loadSnapshot();
  startSync();
  if (state.route.view === 'branches') ensureGraph();
}

export async function addProject(repo, localPath) {
  const r = await api('projects_add', { repo, localPath: localPath || null });
  stopSync();
  state.snap = null;
  state.drawer = null;
  state.graph = { data: null, loading: false, error: null, sel: null, limit: 400, busy: null, forProject: null };
  await loadProjects();
  return r;
}

export async function removeProject(id) {
  try {
    await api('projects_remove', { id });
    stopSync();
    state.snap = null;
    await loadProjects();
  } catch (e) {
    toast(errorText(e), 'err');
  }
}

export async function setLocalPath(id, path) {
  const list = await api('projects_set_path', { id, path: path || null });
  state.projects = list.projects;
  if (id === state.projectId) {
    state.graph = { data: null, loading: false, error: null, sel: null, limit: 400, busy: null, forProject: null };
    if (state.route.view === 'branches') ensureGraph();
  }
  render();
}

export const currentProject = () => state.projects.find((p) => p.id === state.projectId) || null;

/** Changes the app's language: 'en', 'es', or null to follow the system. */
export async function changeLanguage(code) {
  try {
    state.settings = await api('settings_set', { language: code || null });
  } catch (e) {
    toast(errorText(e), 'err');
    return;
  }
  applyLanguage();
  render();
}

// ---- data: snapshot and sync ---------------------------------------------------

export async function loadSnapshot(force = false) {
  const id = state.projectId;
  if (!id) return;
  set({ syncing: true, syncError: null });
  if (!state.snap || state.snap.projectId !== id) {
    state.snap = null;
    try {
      const cached = await api('snapshot_cached', { project: id });
      if (cached && state.projectId === id && !state.snap) set({ snap: cached });
    } catch {
      /* no cache yet */
    }
  }
  try {
    const snap = await api('snapshot', { project: id, force });
    if (state.projectId !== id) return;
    set({ snap, syncedAt: Date.now(), syncing: false, loadError: null });
  } catch (e) {
    if (state.projectId !== id) return;
    set({ syncing: false, syncError: errorText(e), loadError: state.snap ? null : errorText(e) });
    if (e && (e.code === 'unauthorized' || e.code === 'not_signed_in')) toast(t('GitHub no longer accepts this sign-in. Sign in again in Settings.'), 'err');
  }
}

let timer = null;
export function stopSync() {
  clearInterval(timer);
  timer = null;
}
export function startSync() {
  stopSync();
  const every = Math.max(10, Number(state.settings.syncSeconds) || 30) * 1000;
  timer = setInterval(syncNow, every);
}

/** Asks the core whether GitHub changed; cheap when nothing did. */
export async function syncNow() {
  if (state.phase !== 'ready' || state.syncing || !state.projectId || document.visibilityState === 'hidden') return;
  const id = state.projectId;
  state.syncing = true;
  render();
  try {
    const r = await api('sync', { project: id });
    if (state.projectId !== id) return;
    if (r.changed) {
      state.snap = r.snapshot;
      refreshDrawerIfStale();
    }
    state.syncedAt = Date.now();
    state.syncError = null;
  } catch (e) {
    state.syncError = errorText(e);
  } finally {
    state.syncing = false;
    render();
  }
}

export const taskByNumber = (n) => (state.snap ? state.snap.tasks.find((x) => x.number === n) : null);
export const memberByLogin = (login) => (state.snap ? state.snap.members.find((m) => m.login.toLowerCase() === String(login).toLowerCase()) : null);

function putTask(task) {
  if (!state.snap) return;
  const slim = { ...task };
  delete slim.comments;
  const list = state.snap.tasks;
  const i = list.findIndex((x) => x.number === task.number);
  if (i >= 0) list[i] = { ...list[i], ...slim };
  else list.unshift(slim);
}

const openStatus = (s) => s === 'todo' || s === 'in_progress' || s === 'in_review';
/** Keeps the open/closed totals right between syncs. `was` is null for a new task. */
function recount(was, now) {
  if (!state.snap) return;
  const repo = state.snap.repo;
  if (was == null) {
    if (openStatus(now)) repo.openIssues += 1;
    else repo.closedIssues += 1;
  } else if (openStatus(was) !== openStatus(now)) {
    const d = openStatus(now) ? 1 : -1;
    repo.openIssues = Math.max(0, repo.openIssues + d);
    repo.closedIssues = Math.max(0, repo.closedIssues - d);
  }
}

// ---- tasks ---------------------------------------------------------------------

export async function createTask(data) {
  const task = await api('task_create', { project: state.projectId, ...data });
  putTask(task);
  recount(null, task.status);
  toast(t('Task #{n} created', { n: task.number }));
  render();
  return task;
}

/** Applies a change at once on screen, then on GitHub; rolls back if GitHub refuses. */
export async function updateTask(number, patch) {
  const task = taskByNumber(number);
  const before = task ? { ...task } : null;
  const preview = {};
  if (patch.status) preview.status = patch.status;
  if ('priority' in patch) preview.priority = patch.priority || null;
  if ('due' in patch) preview.due = patch.due || null;
  if (patch.title) preview.title = patch.title;
  if (patch.assignees) {
    preview.assignees = patch.assignees.map((login) => {
      const m = memberByLogin(login);
      return m ? { login: m.login, name: m.name, avatarUrl: m.avatarUrl, url: m.url } : { login, avatarUrl: '', url: '' };
    });
  }
  if (task) Object.assign(task, preview);
  if (before && preview.status) recount(before.status, preview.status);
  const drawer = state.drawer && state.drawer.number === number ? state.drawer : null;
  if (drawer && drawer.detail) Object.assign(drawer.detail, preview);
  render();
  try {
    const fresh = await api('task_update', { project: state.projectId, number, ...patch });
    putTask(fresh);
    if (state.drawer && state.drawer.number === number) state.drawer.detail = fresh;
    render();
    return fresh;
  } catch (e) {
    if (before && preview.status) recount(preview.status, before.status);
    if (task && before) Object.assign(task, before);
    if (state.drawer && state.drawer.number === number) loadDrawer(number);
    toast(errorText(e), 'err');
    render();
    return null;
  }
}

export async function loadDrawer(number) {
  const known = taskByNumber(number);
  state.drawer = { number, detail: known ? { ...known, comments: null } : null, loading: true, error: null, draft: '', posting: false, editBody: null, savingBody: false };
  render();
  try {
    const detail = await api('task_get', { project: state.projectId, number });
    if (!state.drawer || state.drawer.number !== number) return;
    state.drawer.detail = detail;
    state.drawer.loading = false;
    if (taskByNumber(number)) putTask(detail);
  } catch (e) {
    if (!state.drawer || state.drawer.number !== number) return;
    state.drawer.loading = false;
    state.drawer.error = errorText(e);
  }
  render();
}

function refreshDrawerIfStale() {
  const d = state.drawer;
  if (!d || !d.detail || d.loading || d.editBody != null) return;
  const known = taskByNumber(d.number);
  if (known && known.updatedAt !== d.detail.updatedAt) {
    api('task_get', { project: state.projectId, number: d.number })
      .then((detail) => {
        if (state.drawer && state.drawer.number === d.number && state.drawer.editBody == null) {
          state.drawer.detail = detail;
          render();
        }
      })
      .catch(() => {});
  }
}

export async function addComment(body) {
  const d = state.drawer;
  if (!d || !d.detail || !body.trim()) return;
  d.posting = true;
  render();
  try {
    const c = await api('comment_add', { project: state.projectId, number: d.number, taskId: d.detail.id, body });
    if (state.drawer === d) {
      d.detail.comments = [...(d.detail.comments || []), c];
      d.detail.commentCount = (d.detail.commentCount || 0) + 1;
      d.draft = '';
    }
    const listed = taskByNumber(d.number);
    if (listed) listed.commentCount = (listed.commentCount || 0) + 1;
  } catch (e) {
    toast(errorText(e), 'err');
  }
  d.posting = false;
  render();
}

export async function deleteComment(id) {
  const d = state.drawer;
  try {
    await api('comment_delete', { id });
    if (state.drawer === d && d.detail) {
      d.detail.comments = (d.detail.comments || []).filter((c) => c.id !== id);
      d.detail.commentCount = Math.max(0, (d.detail.commentCount || 1) - 1);
    }
  } catch (e) {
    toast(errorText(e), 'err');
  }
  render();
}

export async function saveBody(text) {
  const d = state.drawer;
  if (!d) return;
  d.savingBody = true;
  render();
  const fresh = await updateTask(d.number, { body: text });
  if (state.drawer === d) {
    d.savingBody = false;
    if (fresh) d.editBody = null;
  }
  render();
}

export async function startBranch(number, name) {
  try {
    const r = await api('task_start_branch', { project: state.projectId, number, name: name || null });
    toast(t('Branch "{name}" created on GitHub', { name: r.branch }));
    if (state.drawer && state.drawer.number === number) loadDrawer(number);
    return r;
  } catch (e) {
    toast(errorText(e), 'err');
    return null;
  }
}

export async function linkPull(pull, task, link = true) {
  try {
    const r = await api(link ? 'pull_link' : 'pull_unlink', { project: state.projectId, pull, task });
    toast(link ? t('Pull request #{pull} linked to task #{task}', { pull, task }) + (r.note ? `\n${errorText(r.note)}` : '') : t('Pull request #{pull} unlinked', { pull }));
    await loadSnapshot(true);
    if (state.drawer && state.drawer.number === task) loadDrawer(task);
  } catch (e) {
    toast(errorText(e), 'err');
  }
}

// ---- local git ---------------------------------------------------------------------

export function ensureGraph() {
  const p = currentProject();
  if (!p || !p.localPath) return;
  if (state.graph.forProject === p.id && (state.graph.data || state.graph.loading)) return;
  loadGraph();
}

export async function loadGraph() {
  const p = currentProject();
  if (!p || !p.localPath) return;
  const g = state.graph;
  g.loading = true;
  g.error = null;
  g.forProject = p.id;
  render();
  try {
    const data = await api('git_graph', { project: p.id, limit: g.limit });
    if (state.graph !== g) return;
    g.data = data;
    if (g.sel && g.sel.kind === 'commit' && !data.nodes.some((n) => n.sha === g.sel.sha)) g.sel = null;
  } catch (e) {
    if (state.graph !== g) return;
    g.error = errorText(e);
  }
  g.loading = false;
  render();
}

export async function gitAction(label, method, params, done) {
  const g = state.graph;
  if (g.busy) return;
  g.busy = label;
  render();
  try {
    await api(method, { project: state.projectId, ...params });
    if (done) toast(done);
    await loadGraph();
  } catch (e) {
    toast(errorText(e), 'err');
  }
  g.busy = null;
  render();
}
