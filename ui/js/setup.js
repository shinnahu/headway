// The setup window: choose where and how, watch it happen, finish.
// The work itself is done by the Rust side (crates/setup, crates/install).
import { h, mount } from './dom.js';
import { icon } from './icons.js';
import { t, N, setLanguage, language, locale, tError } from './i18n.js';
import { invoke, listen, query, PREVIEW, CAN_PICK_FOLDER, pickFolder, closeWindow, setTitle, wait, windowBar, brand, switchRow, languageChoice, installKeys } from './installer.js';

// The parts of an installation, as the Rust side names them.
const PARTS = [
  ['unpack', N('Unpack the program')],
  ['copy', N('Copy it to its folder')],
  ['shortcuts', N('Create the shortcuts')],
  ['register', N('Add Headway to Installed apps')],
];

const state = {
  phase: 'loading', // loading | options | working | done | failed
  info: null,
  folder: '',
  desktop: true,
  startMenu: true,
  /** The user chose a language here (otherwise the app keeps following Windows). */
  languagePicked: false,
  /** Index in PARTS of the part being worked on; PARTS.length when all are done. */
  part: 0,
  error: null,
  wasRunning: false,
  openAfter: true,
};

// ---- preview in a browser: made-up answers, chosen with ?case= ------------------------

const CASE = query.get('case') || '';
const SAMPLE_FOLDER = 'C:\\Users\\maya\\AppData\\Local\\Programs\\Headway';
const fake = {
  info: async () => ({
    version: '0.2.0',
    programBytes: CASE === 'empty' ? 0 : 7334912,
    folder: SAMPLE_FOLDER,
    installed: { update: { version: '0.1.0', location: SAMPLE_FOLDER }, same: { version: '0.2.0', location: SAMPLE_FOLDER }, newer: { version: '0.3.0', location: SAMPLE_FOLDER } }[CASE] || null,
    language: null,
  }),
  folder: async ({ picked }) => picked + '\\Headway',
  install: async () => {
    for (const [name] of PARTS) {
      heard(name);
      await wait(380);
      if (CASE === 'fail' && name === 'copy') throw { code: 'in_use', message: 'Headway is running. Close it and try again.' };
    }
    heard('done');
    return { wasRunning: CASE === 'running' };
  },
  launch: async () => null,
};

// ---- what is already there ---------------------------------------------------------------

function compare(a, b) {
  const [x, y] = [a, b].map((v) => String(v).split('.').map((n) => parseInt(n, 10) || 0));
  for (let i = 0; i < 3; i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) < (y[i] || 0) ? -1 : 1;
  return 0;
}

/** new | update | same | newer: what installing would do here. */
function kind() {
  const { installed, version } = state.info;
  if (!installed) return 'new';
  const c = compare(installed.version, version);
  return c < 0 ? 'update' : c === 0 ? 'same' : 'newer';
}

const megabytes = (bytes) => (bytes / 1048576).toLocaleString(locale(), { maximumFractionDigits: 1 }) + ' MB';

// ---- actions ---------------------------------------------------------------------------------

function pickLanguage(code) {
  setLanguage(code);
  state.languagePicked = true;
  setTitle(t('Headway Setup'));
  render();
}

async function changeFolder() {
  const picked = await pickFolder(t('Choose where to install Headway'), state.folder);
  if (!picked) return;
  state.folder = await invoke('setup_folder', { picked }, fake.folder);
  render();
}

// The parts arrive faster than anyone can read; each one stays on screen a moment.
const heardParts = [];
let showing = false;
function heard(name) {
  heardParts.push(name);
  if (!showing) showParts();
}
async function showParts() {
  showing = true;
  while (heardParts.length) {
    const name = heardParts.shift();
    const i = PARTS.findIndex(([key]) => key === name);
    state.part = name === 'done' ? PARTS.length : i >= 0 ? i : state.part;
    render();
    await wait(380);
  }
  showing = false;
}
const shownAll = async () => {
  while (showing || heardParts.length) await wait(50);
};

async function install() {
  if (state.phase !== 'options' && state.phase !== 'failed') return;
  if (!state.info.programBytes) return;
  state.phase = 'working';
  state.part = 0;
  state.error = null;
  render();
  try {
    const request = { folder: state.folder, desktopShortcut: state.desktop, startMenuShortcut: state.startMenu, language: state.languagePicked ? language() : null };
    const done = await invoke('setup_install', { request }, fake.install);
    await shownAll();
    await wait(250);
    state.wasRunning = !!done.wasRunning;
    // Opening it now would put a second Headway next to the one still running.
    state.openAfter = !done.wasRunning;
    state.phase = 'done';
  } catch (e) {
    await shownAll();
    state.error = tError(e);
    state.phase = 'failed';
  }
  render();
}

async function finish() {
  if (state.openAfter) {
    try {
      await invoke('setup_launch', {}, fake.launch);
    } catch (e) {
      state.error = tError(e);
      state.openAfter = false;
      return render();
    }
  }
  closeWindow();
}

// ---- screens ------------------------------------------------------------------------------------

const TITLES = {
  new: N('Install Headway'),
  update: N('Update Headway'),
  same: N('Headway {version} is already installed'),
  newer: N('A newer Headway is installed'),
};
const LEADS = {
  new: N('It is installed for your Windows user only: no administrator rights are needed, and nothing is downloaded.'),
  update: N('Version {old} is installed. This updates it to {version}. Your settings and your sign-in stay as they are.'),
  same: N('Installing it again repairs the program and its shortcuts. Your settings and your sign-in stay as they are.'),
  newer: N('Version {old} is installed. This replaces it with the older {version}. Your settings and your sign-in stay as they are.'),
};
const GO = { new: N('Install'), update: N('Update'), same: N('Install again'), newer: N('Install {version}') };

function options() {
  const { info } = state;
  const k = kind();
  const vars = { version: info.version, old: info.installed ? info.installed.version : '' };
  return [
    h('h1', null, t(TITLES[k], vars)),
    h('p', { class: 'lead' }, t(LEADS[k], vars)),
    info.programBytes ? null : h('div', { class: 'error-box' }, t('This setup was built without the program inside, so it cannot install anything. Build it with scripts\\release.ps1.')),
    h('div', { class: 'card' },
      h('div', { class: 'row' },
        h('span', { class: 'ic' }, icon('folder', 16)),
        h('span', { class: 'txt' }, h('b', null, t('Folder')), h('small', { class: 'path', title: state.folder }, h('bdi', null, state.folder))),
        CAN_PICK_FOLDER || PREVIEW ? h('button', { class: 'btn sm', type: 'button', onclick: changeFolder }, t('Change…')) : null),
      switchRow('home', t('Shortcut on the desktop'), null, state.desktop, (on) => { state.desktop = on; render(); }),
      switchRow('list', t('Shortcut in the Start menu'), null, state.startMenu, (on) => { state.startMenu = on; render(); }),
      h('div', { class: 'row' },
        h('span', { class: 'ic' }, icon('globe', 16)),
        h('span', { class: 'txt' }, h('b', null, t('Language')), h('small', null, t('Of this setup and of Headway. It can be changed later in Settings.'))),
        languageChoice(pickLanguage))),
    h('div', { class: 'foot' },
      h('span', { class: 'meta' }, info.programBytes ? megabytes(info.programBytes) : ''),
      h('button', { class: 'btn quiet', type: 'button', onclick: closeWindow }, t('Cancel')),
      h('button', { class: 'btn primary', type: 'button', 'data-primary': '', disabled: !info.programBytes, onclick: install }, icon('download', 16), t(GO[k], vars))),
  ];
}

function working() {
  const updating = kind() !== 'new';
  return [
    h('h1', null, updating ? t('Updating Headway…') : t('Installing Headway…')),
    h('p', { class: 'lead' }, t('This takes a few seconds.')),
    h('div', { class: 'bar', role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': PARTS.length, 'aria-valuenow': state.part },
      h('i', { style: { width: Math.round(((state.part + 0.5) / (PARTS.length + 0.5)) * 100) + '%' } })),
    h('ol', { class: 'steps' }, PARTS.map(([key, label], i) => {
      const cls = i < state.part ? 'done' : i === state.part ? 'now' : '';
      return h('li', { class: cls, key }, h('span', { class: 'mark' }, i < state.part ? icon('check', 12, { 'stroke-width': 2.6 }) : null), t(label));
    })),
  ];
}

function done() {
  const updated = kind() !== 'new';
  return [
    h('span', { class: 'badge' }, icon('check', 28, { 'stroke-width': 2.2 })),
    h('h1', null, updated ? t('Headway is up to date') : t('Headway is installed')),
    h('p', { class: 'lead' }, state.startMenu ? t('Version {version} is ready. You will find it in the Start menu.', { version: state.info.version }) : t('Version {version} is ready, in {folder}.', { version: state.info.version, folder: state.folder })),
    state.wasRunning ? h('div', { class: 'note' }, icon('info', 16), h('span', null, t('Headway was running during the update. That copy keeps working as it was; the new version is used the next time Headway starts.'))) : null,
    state.error ? h('div', { class: 'error-box' }, state.error) : null,
    h('div', { class: 'card' }, switchRow('external', t('Open Headway now'), null, state.openAfter, (on) => { state.openAfter = on; render(); })),
    h('div', { class: 'foot' }, h('button', { class: 'btn primary', type: 'button', 'data-primary': '', onclick: finish }, t('Finish'))),
  ];
}

function failed() {
  return [
    h('span', { class: 'badge bad' }, icon('alert', 28)),
    h('h1', null, t('Headway was not installed')),
    h('div', { class: 'error-box' }, state.error),
    h('div', { class: 'foot' },
      h('button', { class: 'btn quiet', type: 'button', onclick: () => { state.phase = 'options'; render(); } }, t('Back')),
      h('button', { class: 'btn primary', type: 'button', 'data-primary': '', onclick: install }, t('Try again'))),
  ];
}

function view() {
  const { phase, info } = state;
  const body = phase === 'options' ? options() : phase === 'working' ? working() : phase === 'done' ? done() : phase === 'failed' ? failed() : state.error ? [h('span', { class: 'badge bad' }, icon('alert', 28)), h('h1', null, t('Headway Setup could not start')), h('div', { class: 'error-box' }, state.error)] : null;
  return h('div', { class: 'win' },
    windowBar(phase === 'working'),
    h('aside', { class: 'hero' },
      brand(),
      h('div', { class: 'art' }, h('img', { src: 'assets/setup-art.svg', alt: '' })),
      h('h2', null, t('Tasks, pull requests and branches for small teams.')),
      h('div', { class: 'ver' }, info ? t('Version {version}', { version: info.version }) : '')),
    h('main', { class: 'pane', key: 'pane-' + phase }, body));
}

const root = document.getElementById('app');
const render = () => mount(root, view());

// Enter does what the main button of the screen does.
installKeys(
  () => { if (state.phase === 'options' || state.phase === 'failed') install(); else if (state.phase === 'done') finish(); },
  () => { if (state.phase !== 'working') closeWindow(); },
);
listen('setup-step', heard);

(async () => {
  setLanguage(query.get('lang'));
  render();
  try {
    state.info = await invoke('setup_info', {}, fake.info);
    state.folder = state.info.folder;
    // The app's own language if it has one; otherwise the language of Windows.
    setLanguage(query.get('lang') || state.info.language);
    state.phase = 'options';
  } catch (e) {
    state.error = tError(e);
  }
  setTitle(t('Headway Setup'));
  // ?phase=working|done|failed shows that screen at once (for previews and pictures).
  const jump = PREVIEW ? query.get('phase') : null;
  if (jump && state.info) {
    state.phase = jump;
    state.part = Number(query.get('part') || 2);
    state.wasRunning = CASE === 'running';
    if (jump === 'failed') state.error = tError('Headway is running. Close it and try again.');
  }
  render();
})();
