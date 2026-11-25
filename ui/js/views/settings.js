// Settings: account, language, projects, calendars, sync, sign-in setup, Claude (MCP).
import { h } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, set, signOut, openModal, removeProject, setLocalPath, selectProject, startSync, toast, removeCalendar, changeLanguage } from '../store.js';
import { avatar, spinner } from '../components.js';
import { api, openUrl, copyText, pickFolder, HAS_FOLDER_PICKER, MOCK } from '../api.js';
import { t, N, LANGUAGES, systemLanguage } from '../i18n.js';
import * as U from '../util.js';

const local = { editPath: null, pathValue: '', pathBusy: false, clientId: null, clientBusy: false };

const SYNC_CHOICES = [[15, N('Every 15 seconds')], [30, N('Every 30 seconds')], [60, N('Every minute')], [120, N('Every 2 minutes')], [300, N('Every 5 minutes')]];

async function savePath(id, path) {
  local.pathBusy = true;
  render();
  try {
    await setLocalPath(id, path);
    local.editPath = null;
    toast(path ? t('Local folder saved') : t('Local folder removed'));
  } catch (e) {
    toast(U.errorText(e), 'err');
  }
  local.pathBusy = false;
  render();
}

async function choosePath(p) {
  if (HAS_FOLDER_PICKER) {
    const picked = await pickFolder();
    if (picked) savePath(p.id, picked);
  } else {
    local.editPath = p.id;
    local.pathValue = p.localPath || '';
    render();
  }
}

function projectRow(p) {
  const editing = local.editPath === p.id;
  return h('div', { class: 'set-row', key: p.id },
    icon('repo', 18),
    h('div', { class: 'grow' },
      h('b', null, p.id, p.id === state.projectId ? h('span', { class: 'tag', style: { 'margin-left': '8px' } }, t('Current')) : null),
      editing
        ? h('form', { class: 'row', style: { 'margin-top': '6px' }, onsubmit: (e) => { e.preventDefault(); savePath(p.id, local.pathValue.trim()); } },
            h('input', { class: 'input grow', placeholder: t('Folder of your local clone'), spellcheck: 'false', value: local.pathValue, oninput: (e) => { local.pathValue = e.target.value; } }),
            h('button', { class: 'btn sm primary', type: 'submit', disabled: local.pathBusy }, local.pathBusy ? spinner() : t('Save')),
            h('button', { class: 'btn sm quiet', type: 'button', onclick: () => { local.editPath = null; render(); } }, t('Cancel')))
        : h('small', null, p.localPath ? t('Local folder: {path}', { path: p.localPath }) : t('No local folder (the Branches screen needs one)'))),
    editing ? null : h('button', { class: 'btn sm', type: 'button', onclick: () => choosePath(p) }, icon('folder', 14), p.localPath ? t('Change folder') : t('Set folder')),
    !editing && p.localPath ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => savePath(p.id, '') }, t('Forget folder')) : null,
    !editing && p.id !== state.projectId ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => selectProject(p.id) }, t('Open project')) : null,
    editing ? null : h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': t('Remove project'), 'data-tip': t('Remove from Headway'), onclick: () => openModal({ kind: 'confirm', title: t('Remove {repo} from Headway?', { repo: p.id }), text: t('This only removes it from this app. Nothing changes on GitHub: the tasks, pull requests and branches all stay.'), confirmLabel: t('Remove'), danger: true, onConfirm: () => removeProject(p.id) }) }, icon('trash', 15)));
}

async function saveSync(seconds) {
  try {
    const s = await api('settings_set', { syncSeconds: seconds });
    set({ settings: s });
    startSync();
  } catch (e) {
    toast(U.errorText(e), 'err');
  }
}

async function saveClientId() {
  local.clientBusy = true;
  render();
  try {
    const s = await api('settings_set', { githubClientId: (local.clientId || '').trim() || null });
    state.settings = s;
    if (state.auth) state.auth.deviceFlowConfigured = !!s.githubClientId;
    toast(s.githubClientId ? t('Client ID saved') : t('Client ID removed'));
  } catch (e) {
    toast(U.errorText(e), 'err');
  }
  local.clientBusy = false;
  render();
}

function languagePanel() {
  const chosen = state.settings.language || '';
  const system = LANGUAGES.find(([code]) => code === systemLanguage());
  return h('div', { class: 'panel' },
    h('div', { class: 'panel-head' }, t('Language')),
    h('div', { class: 'set-row' },
      icon('globe', 18),
      h('div', { class: 'grow' }, h('b', null, t('Language of the app')), h('small', null, t('Changes right away. "Automatic" follows the language of Windows.'))),
      h('select', { class: 'select', style: { width: '210px' }, 'aria-label': t('Language of the app'), onchange: (e) => changeLanguage(e.target.value || null) },
        h('option', { value: '', selected: !chosen }, t('Automatic ({language})', { language: system ? system[1] : 'English' })),
        LANGUAGES.map(([code, name]) => h('option', { value: code, selected: chosen === code }, name)))));
}

export function settingsView() {
  const u = state.user;
  const auth = state.auth || {};
  const info = state.info || {};
  if (local.clientId == null) local.clientId = state.settings.githubClientId || '';
  // The MCP server is the same program started with --mcp.
  const exe = info.exePath;
  const mcpCommand = exe ? `claude mcp add --scope user headway -- "${exe}" --mcp` : null;
  const mcpJson = exe ? JSON.stringify({ mcpServers: { headway: { command: exe, args: ['--mcp'] } } }, null, 2) : null;
  const copy = async (text, done) => { if (await copyText(text)) toast(done); };

  return h('div', { class: 'settings' },
    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, t('Account')),
      u ? h('div', { class: 'set-row' }, avatar(u, 40),
        h('div', { class: 'grow' }, h('b', null, U.displayName(u)), h('small', null, `@${u.login} · ` + (auth.source === 'gh' ? t('signed in through the GitHub CLI on this computer') : t('token kept in Windows Credential Manager')))),
        h('button', { class: 'btn sm', type: 'button', onclick: () => openUrl(u.url) }, icon('external', 14), t('Profile')),
        h('button', { class: 'btn sm danger', type: 'button', onclick: signOut }, icon('logout', 14), t('Sign out'))) : null),

    languagePanel(),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, t('Projects'), h('span', { class: 'right' }, h('button', { class: 'btn sm primary', type: 'button', onclick: () => openModal({ kind: 'addProject' }) }, icon('plus', 14), t('Add a project')))),
      state.projects.length ? state.projects.map(projectRow) : h('div', { class: 'set-row muted' }, t('No projects yet.'))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, t('Calendars'), h('span', { class: 'right' }, h('button', { class: 'btn sm primary', type: 'button', onclick: () => openModal({ kind: 'connectCalendar' }) }, icon('plus', 14), t('Connect a calendar')))),
      state.cal.list.length
        ? state.cal.list.map((c) => h('div', { class: 'set-row', key: 'cal-' + c.id }, icon('calendar', 18),
            h('div', { class: 'grow' }, h('b', null, c.name), h('small', null, t('Read-only, through its private link. The link is kept in Windows Credential Manager.'))),
            h('button', { class: 'btn sm danger', type: 'button', onclick: () => openModal({ kind: 'confirm', title: t('Disconnect "{name}"?', { name: c.name }), text: t('Its meetings stop showing in the Agenda and the stored link is deleted from this computer. Nothing changes in Google Calendar.'), confirmLabel: t('Disconnect'), danger: true, onConfirm: () => removeCalendar(c.id) }) }, t('Disconnect'))))
        : h('div', { class: 'set-row' }, h('div', { class: 'grow' }, h('b', null, t('No calendar connected')), h('small', null, t('Connect a Google Calendar by its private link to see your meetings in the Agenda. Read-only; each person connects their own.'))))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, t('Sync')),
      h('div', { class: 'set-row' },
        h('div', { class: 'grow' }, h('b', null, t('Check GitHub for changes')), h('small', null, t('There is no Headway server: the app asks GitHub whether anything changed. When nothing did, the check is free and does not count against GitHub\'s limit.'))),
        h('select', { class: 'select', style: { width: '210px' }, 'aria-label': t('How often to check'), onchange: (e) => saveSync(Number(e.target.value)) },
          SYNC_CHOICES.map(([v, label]) => h('option', { value: v, selected: Number(state.settings.syncSeconds) === v }, t(label)))))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, t('Sign in with a code (for teammates without the GitHub CLI)')),
      h('div', { class: 'set-row' },
        h('div', { class: 'grow' },
          h('b', null, t('GitHub OAuth App Client ID')),
          h('small', null, t('One person registers an OAuth App on GitHub once (Settings, Developer settings, OAuth Apps), turns on "Device flow", and shares its Client ID with the team. It is not a secret.')),
          h('form', { class: 'row', style: { 'margin-top': '8px' }, onsubmit: (e) => { e.preventDefault(); saveClientId(); } },
            h('input', { class: 'input grow', placeholder: t('For example Ov23li…'), spellcheck: 'false', autocomplete: 'off', value: local.clientId, oninput: (e) => { local.clientId = e.target.value; } }),
            h('button', { class: 'btn sm', type: 'submit', disabled: local.clientBusy }, local.clientBusy ? spinner() : t('Save')))))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'Claude'),
      h('div', { class: 'set-row' },
        h('div', { class: 'grow' },
          h('b', null, t('Let Claude read and update tasks')),
          h('small', null, t('Headway has an MCP server built in. Once added to Claude, it can list tasks, change status, priority, due date and assignees, comment, link pull requests and report the project status, using this same sign-in and project list.')),
          mcpCommand
            ? [
                h('div', { class: 'label', style: { margin: '12px 0 6px' } }, t('Claude Code: run this once in a terminal')),
                h('div', { class: 'code-line' }, h('span', null, mcpCommand), h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': t('Copy command'), onclick: () => copy(mcpCommand, t('Command copied')) }, icon('copy', 14))),
                h('div', { class: 'label', style: { margin: '12px 0 6px' } }, t('Claude Desktop: add this to claude_desktop_config.json')),
                h('div', { class: 'code-line', style: { 'white-space': 'pre', 'align-items': 'flex-start' } }, h('span', null, mcpJson), h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': t('Copy JSON'), onclick: () => copy(mcpJson, t('JSON copied')) }, icon('copy', 14))),
              ]
            : h('small', { style: { 'margin-top': '8px' } }, t('The lines to copy appear here when Headway runs as the installed program.'))))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, t('About')),
      h('div', { class: 'set-row' }, h('div', { class: 'grow' }, h('b', null, `Headway ${info.version || ''}`), h('small', null, MOCK ? t('Demo data: nothing here is real or saved.') : t('Settings file: {path}', { path: info.configPath || '' }))))));
}
