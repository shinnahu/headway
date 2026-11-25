// The uninstall window (headway.exe --uninstall): confirm, remove, done.
import { h, mount } from './dom.js';
import { icon } from './icons.js';
import { t, setLanguage, tError } from './i18n.js';
import { invoke, query, PREVIEW, closeWindow, setTitle, wait, windowBar, brand, switchRow, installKeys } from './installer.js';

const state = {
  phase: 'loading', // loading | confirm | working | done | failed | absent
  info: null,
  /** Also delete the settings, the saved sign-in and the cached data. */
  removeData: false,
  problems: [],
  error: null,
};

// Preview in a browser: made-up answers, chosen with ?case=
const CASE = query.get('case') || '';
const fake = {
  info: async () => ({ version: '0.2.0', installed: CASE === 'absent' ? null : { version: '0.2.0', location: 'C:\\Users\\maya\\AppData\\Local\\Programs\\Headway' }, language: null }),
  run: async () => {
    await wait(900);
    if (CASE === 'fail') throw { code: 'failed', message: 'Could not delete the program: Access is denied. (os error 5)' };
    return { problems: CASE === 'problems' ? ['Could not delete the settings: The process cannot access the file because it is being used by another process. (os error 32)'] : [] };
  },
};

async function uninstall() {
  if (state.phase !== 'confirm' && state.phase !== 'failed') return;
  state.phase = 'working';
  state.error = null;
  render();
  try {
    const [result] = await Promise.all([invoke('uninstall_run', { removeData: state.removeData }, fake.run), wait(700)]);
    state.problems = (result.problems || []).map((p) => tError(p));
    state.phase = 'done';
  } catch (e) {
    state.error = tError(e);
    state.phase = 'failed';
  }
  render();
}

function confirm() {
  return [
    h('h1', null, t('Uninstall Headway?')),
    h('p', { class: 'lead' }, t('This removes the program, its shortcuts and its entry in Installed apps. Your tasks, pull requests and branches are on GitHub and stay there.')),
    h('div', { class: 'card' },
      switchRow('trash', t('Also delete my settings'), t('The list of projects, the saved sign-in, calendar links and cached data on this computer.'), state.removeData, (on) => { state.removeData = on; render(); })),
    h('p', { class: 'quiet-text' }, t('If Headway is running, it is closed first.')),
    h('div', { class: 'foot' },
      h('button', { class: 'btn quiet', type: 'button', 'data-primary': '', onclick: closeWindow }, t('Cancel')),
      h('button', { class: 'btn danger', type: 'button', onclick: uninstall }, t('Uninstall'))),
  ];
}

function working() {
  return [
    h('h1', null, t('Removing Headway…')),
    h('div', { class: 'busy' }, h('span', { class: 'spinner' }), t('This takes a few seconds.')),
  ];
}

function done() {
  return [
    h('span', { class: 'badge' }, icon('check', 28, { 'stroke-width': 2.2 })),
    h('h1', null, t('Headway was removed')),
    h('p', { class: 'lead' }, state.removeData && !state.problems.length ? t('The program and your settings are gone from this computer.') : t('The program is gone from this computer.')),
    state.problems.length ? h('div', { class: 'error-box' }, t('Some things could not be deleted:'), h('ul', null, state.problems.map((p) => h('li', null, p)))) : null,
    h('div', { class: 'foot' }, h('button', { class: 'btn primary', type: 'button', 'data-primary': '', onclick: closeWindow }, t('Close'))),
  ];
}

function failed() {
  return [
    h('span', { class: 'badge bad' }, icon('alert', 28)),
    h('h1', null, t('Headway could not be removed')),
    h('div', { class: 'error-box' }, state.error),
    h('div', { class: 'foot' },
      h('button', { class: 'btn quiet', type: 'button', onclick: closeWindow }, t('Close')),
      h('button', { class: 'btn primary', type: 'button', 'data-primary': '', onclick: uninstall }, t('Try again'))),
  ];
}

function absent() {
  return [
    h('span', { class: 'badge plain' }, icon('info', 28)),
    h('h1', null, t('Headway is not installed')),
    h('p', { class: 'lead' }, t('There is nothing to remove for this Windows user.')),
    h('div', { class: 'foot' }, h('button', { class: 'btn primary', type: 'button', 'data-primary': '', onclick: closeWindow }, t('Close'))),
  ];
}

const SCREENS = { confirm, working, done, failed, absent };

const root = document.getElementById('app');
function render() {
  const screen = SCREENS[state.phase];
  mount(root, h('div', { class: 'win single' },
    windowBar(state.phase === 'working'),
    h('main', { class: 'pane', key: 'pane-' + state.phase }, brand(), screen ? screen() : state.error ? h('div', { class: 'error-box' }, state.error) : null)));
}

// On the question, Enter does nothing: removing is never the default answer.
installKeys(
  () => { if (state.phase === 'failed') uninstall(); else if (state.phase === 'done' || state.phase === 'absent') closeWindow(); },
  () => { if (state.phase !== 'working') closeWindow(); },
);

(async () => {
  setLanguage(query.get('lang'));
  render();
  try {
    state.info = await invoke('uninstall_info', {}, fake.info);
    setLanguage(query.get('lang') || state.info.language);
    state.phase = state.info.installed ? 'confirm' : 'absent';
  } catch (e) {
    state.error = tError(e);
  }
  setTitle(t('Uninstall Headway'));
  const jump = PREVIEW ? query.get('phase') : null;
  if (jump && state.info) {
    state.phase = jump;
    if (jump === 'failed') state.error = tError('Could not delete the program: Access is denied. (os error 5)');
    if (jump === 'done' && CASE === 'problems') state.problems = [tError('Could not delete the settings: Access is denied. (os error 5)')];
  }
  render();
})();
