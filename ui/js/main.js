// App shell: sidebar, top bar, the current view, and the layers above it.
import { h, mount, cx } from './dom.js';
import { icon, iconFilled } from './icons.js';
import {
  state, setRenderer, render, set, boot, applyRoute, go, openMenu, closeMenu, closeModal, closeTask,
  selectProject, openModal, signOut, loadSnapshot, syncNow, currentProject,
} from './store.js';
import { avatar, menuView, fitMenu, toastsView, installTooltips, spinner, empty } from './components.js';
import { SHOWCASE, openUrl } from './api.js';
import { t, tn, N } from './i18n.js';
import * as U from './util.js';
import { healthView } from './views/health.js';
import { tasksView } from './views/tasks.js';
import { overviewView } from './views/overview.js';
import { teamView } from './views/team.js';
import { branchesView, afterBranchesDraw } from './views/branches.js';
import { agendaView } from './views/agenda.js';
import { settingsView } from './views/settings.js';
import { gateView } from './views/gate.js';
import { drawerView } from './views/drawer.js';
import { modalView, focusModal } from './views/modals.js';

const TITLES = { overview: N('Overview'), tasks: N('Tasks'), agenda: N('Agenda'), health: N('Project health'), branches: N('Branches'), team: N('Team'), settings: N('Settings') };
const VIEW = { overview: overviewView, tasks: tasksView, agenda: agendaView, health: healthView, branches: branchesView, team: teamView, settings: settingsView };

function navItem(view, iconName, extra) {
  const active = state.route.view === view;
  const label = t(TITLES[view]);
  return h('a', { class: cx('nav-item', active && 'active'), href: '#/' + view, 'aria-current': active ? 'page' : null, 'data-tip': state.collapsed ? label : null },
    active && view === 'health' ? iconFilled('health', 19) : icon(iconName, 19),
    h('span', null, label),
    extra || null);
}

function projectMenu(anchor) {
  const items = state.projects.map((p) => ({ label: p.repo, sub: p.owner, icon: 'repo', checked: p.id === state.projectId, onClick: () => selectProject(p.id) }));
  items.push('sep', { label: t('Add a project…'), icon: 'plus', onClick: () => openModal({ kind: 'addProject' }) });
  openMenu(anchor, items, { matchWidth: true });
}

function syncLine() {
  let cls = 'sync';
  let text;
  if (state.syncing) {
    cls += ' busy';
    text = t('Checking GitHub…');
  } else if (state.syncError) {
    cls += ' err';
    text = t('Could not reach GitHub');
  } else if (state.syncedAt) {
    text = t('Up to date · {when}', { when: U.ago(new Date(state.syncedAt).toISOString()) });
  } else {
    text = t('Not synced yet');
  }
  const tip = state.syncError ? U.errorText(state.syncError) : tn(state.settings.syncSeconds || 30, 'Headway asks GitHub for changes every {n} second', 'Headway asks GitHub for changes every {n} seconds');
  return h('div', { class: cls, 'data-tip': tip }, h('i', { class: 'dot' }), h('span', null, text));
}

function userMenu(anchor) {
  openMenu(anchor, [
    { label: t('Settings'), icon: 'settings', onClick: () => go('settings') },
    { label: t('Open my GitHub profile'), icon: 'external', onClick: () => openUrl(state.user.url) },
    'sep',
    { label: t('Sign out'), icon: 'logout', danger: true, onClick: signOut },
  ], { matchWidth: true });
}

function sidebar() {
  const snap = state.snap;
  const p = currentProject();
  const open = snap ? snap.tasks.filter(U.isOpen) : [];
  const overdue = open.filter((x) => x.due && U.daysUntil(x.due) < 0).length;
  const u = state.user;
  return h('aside', { class: 'sidebar' },
    h('div', { class: 'brand' },
      h('img', { src: 'assets/logo.svg', alt: '' }),
      h('b', { class: 'grow' }, 'Headway'),
      h('button', { class: 'icon-btn', type: 'button', 'aria-label': t('Collapse the sidebar'), onclick: () => set({ collapsed: !state.collapsed }) }, icon('sidebar', 17))),
    h('button', { class: 'project-switch', type: 'button', 'data-tip': state.collapsed && p ? p.id : null, onclick: (e) => projectMenu(e.currentTarget) },
      h('span', { class: 'repo-ic' }, icon('repo', 16)),
      h('span', { class: 'grow' }, h('small', { class: 'truncate' }, p ? p.owner : t('Project')), h('span', { class: 'truncate' }, p ? p.repo : t('Choose a project'))),
      icon('chevDown', 15, { class: 'chev' })),
    h('nav', { class: 'nav' },
      h('div', { class: 'nav-label' }, t('Work')),
      navItem('overview', 'overview'),
      navItem('tasks', 'tasks', snap && snap.repo.openIssues ? h('span', { class: 'count' }, String(snap.repo.openIssues)) : null),
      navItem('agenda', 'agenda', overdue ? h('span', { class: 'count warn', 'data-tip': tn(overdue, '{n} overdue task', '{n} overdue tasks') }, String(overdue)) : null),
      h('div', { class: 'nav-sep' }),
      h('div', { class: 'nav-label' }, t('Project')),
      navItem('health', 'health'),
      navItem('branches', 'branches'),
      navItem('team', 'team'),
      h('div', { class: 'nav-sep' }),
      h('div', { class: 'nav-label' }, t('App')),
      navItem('settings', 'settings')),
    h('div', { class: 'side-foot' },
      syncLine(),
      u ? h('button', { class: 'me', type: 'button', onclick: (e) => userMenu(e.currentTarget) }, avatar(u, 32), h('span', { class: 'grow' }, h('b', { class: 'truncate' }, U.displayName(u)), h('small', { class: 'truncate' }, '@' + u.login))) : null));
}

function topbar() {
  const v = state.route.view;
  return h('header', { class: 'topbar' },
    h('h1', null, t(TITLES[v])),
    h('span', { class: 'grow' }),
    h('label', { class: 'search' }, icon('search', 17),
      h('input', {
        id: 'global-search', placeholder: t('Search tasks…'), 'aria-label': t('Search tasks'), value: state.tasksUi.search,
        oninput: (e) => {
          state.tasksUi.search = e.target.value;
          if (state.route.view !== 'tasks') go('tasks');
          else render();
        },
      }),
      h('kbd', null, 'Ctrl K')),
    h('button', { class: 'icon-btn', type: 'button', 'aria-label': t('Refresh from GitHub'), 'data-tip': t('Refresh from GitHub'), onclick: () => loadSnapshot(true) }, state.syncing ? spinner() : icon('refresh', 17)),
    h('button', { class: 'btn primary', type: 'button', onclick: () => openModal({ kind: 'newTask' }) }, icon('plus', 16), t('New task')));
}

function content() {
  const v = state.route.view;
  if (v === 'settings') return settingsView();
  if (!state.snap) {
    if (state.loadError) {
      return h('div', { class: 'well', style: { 'margin-top': '8px' } }, empty('alert', t('Could not load this project'), U.errorText(state.loadError), h('button', { class: 'btn', type: 'button', style: { 'margin-top': '10px' }, onclick: () => loadSnapshot(true) }, icon('refresh', 16), t('Try again'))));
    }
    return h('div', { class: 'loading' }, h('div', { class: 'row' }, spinner(), t('Loading from GitHub…')));
  }
  const banner = state.syncError
    ? h('div', { class: 'banner err' }, icon('alert', 17), h('span', null, t('Showing the last data Headway loaded.'), ' ', U.errorText(state.syncError)), h('button', { class: 'btn sm right', type: 'button', onclick: () => loadSnapshot(true) }, t('Retry')))
    : state.snap.tasksTruncated
      ? h('div', { class: 'banner warn' }, icon('info', 17), h('span', null, t('This repository has more open issues than Headway loads (500). The newest ones are shown.')))
      : !state.snap.repo.hasIssues
        ? h('div', { class: 'banner warn' }, icon('info', 17), h('span', null, t('Issues are turned off for this repository on GitHub, so it cannot hold tasks. Turn them on in the repository settings.')))
        : null;
  return h('div', { class: 'view', key: 'view-' + v }, banner, VIEW[v]());
}

function appView() {
  if (state.phase === 'boot') return h('div', { class: 'loading', key: 'boot' }, h('div', { class: 'row' }, spinner(), t('Starting Headway…')));
  if (state.phase === 'signin' || state.phase === 'project') return gateView();
  return h('div', { class: cx('shell', state.collapsed && 'collapsed'), key: 'shell' },
    sidebar(),
    h('main', { class: 'main' }, topbar(), h('div', { class: 'content', id: 'content' }, content())));
}

function layersView() {
  const ready = state.phase === 'ready';
  return h('div', { class: 'layers' },
    // The task panel needs the project's data (members, pull requests) to be there.
    ready && state.drawer && state.snap ? drawerView() : null,
    state.modal ? modalView() : null,
    state.menu ? menuView() : null,
    toastsView());
}

const appRoot = document.getElementById('app');
const layerRoot = document.getElementById('layer');

setRenderer(() => {
  mount(appRoot, appView());
  mount(layerRoot, layersView());
  fitMenu();
  focusModal();
  afterBranchesDraw();
});

window.addEventListener('hashchange', applyRoute);
window.addEventListener('resize', render);
window.addEventListener('focus', () => syncNow());
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    if (state.menu) closeMenu();
    else if (state.modal) closeModal();
    else if (state.drawer) closeTask();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    const box = document.getElementById('global-search');
    if (box) box.focus();
  }
});
// Web links always open in the system browser, never inside the app window.
document.addEventListener('click', (e) => {
  const a = e.target instanceof Element ? e.target.closest('a[href]') : null;
  if (!a) return;
  const href = a.getAttribute('href') || '';
  if (/^https?:\/\//i.test(href)) {
    e.preventDefault();
    openUrl(href);
  }
});

if (SHOWCASE || new URLSearchParams(location.search).has('still')) document.body.classList.add('still');
if (SHOWCASE) {
  document.body.classList.add('showcase');
  const zoom = Number(new URLSearchParams(location.search).get('zoom')) || 1.22;
  appRoot.style.transform = `scale(${zoom})`;
}
installTooltips();
state.route = { view: 'health', task: null };
render();
boot();
// Keep "synced 2 minutes ago" honest.
setInterval(() => {
  if (state.phase === 'ready' && !state.menu && !state.modal) render();
}, 30000);
