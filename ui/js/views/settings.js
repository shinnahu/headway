// Settings: account, projects, sync, sign-in setup, Claude (MCP).
import { h } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, set, signOut, openModal, removeProject, setLocalPath, selectProject, startSync, toast, removeCalendar } from '../store.js';
import { avatar, spinner } from '../components.js';
import { api, openUrl, copyText, pickFolder, HAS_FOLDER_PICKER, MOCK } from '../api.js';
import * as U from '../util.js';

const local = { editPath: null, pathValue: '', pathBusy: false, clientId: null, clientBusy: false };

async function savePath(id, path) {
  local.pathBusy = true;
  render();
  try {
    await setLocalPath(id, path);
    local.editPath = null;
    toast(path ? 'Local folder saved' : 'Local folder removed');
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
      h('b', null, p.id, p.id === state.projectId ? h('span', { class: 'tag', style: { 'margin-left': '8px' } }, 'Current') : null),
      editing
        ? h('form', { class: 'row', style: { 'margin-top': '6px' }, onsubmit: (e) => { e.preventDefault(); savePath(p.id, local.pathValue.trim()); } },
            h('input', { class: 'input grow', placeholder: 'Folder of your local clone', spellcheck: 'false', value: local.pathValue, oninput: (e) => { local.pathValue = e.target.value; } }),
            h('button', { class: 'btn sm primary', type: 'submit', disabled: local.pathBusy }, local.pathBusy ? spinner() : 'Save'),
            h('button', { class: 'btn sm quiet', type: 'button', onclick: () => { local.editPath = null; render(); } }, 'Cancel'))
        : h('small', null, p.localPath ? 'Local folder: ' + p.localPath : 'No local folder (the Branches screen needs one)')),
    editing ? null : h('button', { class: 'btn sm', type: 'button', onclick: () => choosePath(p) }, icon('folder', 14), p.localPath ? 'Change folder' : 'Set folder'),
    !editing && p.localPath ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => savePath(p.id, '') }, 'Forget folder') : null,
    !editing && p.id !== state.projectId ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => selectProject(p.id) }, 'Open') : null,
    editing ? null : h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': 'Remove project', 'data-tip': 'Remove from Headway', onclick: () => openModal({ kind: 'confirm', title: `Remove ${p.id} from Headway?`, text: 'This only removes it from this app. Nothing changes on GitHub: the tasks, pull requests and branches all stay.', confirmLabel: 'Remove', danger: true, onConfirm: () => removeProject(p.id) }) }, icon('trash', 15)));
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
    toast(s.githubClientId ? 'Client ID saved' : 'Client ID removed');
  } catch (e) {
    toast(U.errorText(e), 'err');
  }
  local.clientBusy = false;
  render();
}

export function settingsView() {
  const u = state.user;
  const auth = state.auth || {};
  const info = state.info || {};
  if (local.clientId == null) local.clientId = state.settings.githubClientId || '';
  const mcpPath = info.mcpPath;
  const mcpCommand = mcpPath ? `claude mcp add --scope user headway -- "${mcpPath}"` : null;
  const mcpJson = mcpPath ? JSON.stringify({ mcpServers: { headway: { command: mcpPath, args: [] } } }, null, 2) : null;
  const copy = async (text, what) => { if (await copyText(text)) toast(what + ' copied'); };

  return h('div', { class: 'settings' },
    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'Account'),
      u ? h('div', { class: 'set-row' }, avatar(u, 40),
        h('div', { class: 'grow' }, h('b', null, U.displayName(u)), h('small', null, `@${u.login} · ` + (auth.source === 'gh' ? 'signed in through the GitHub CLI on this computer' : 'token kept in Windows Credential Manager'))),
        h('button', { class: 'btn sm', type: 'button', onclick: () => openUrl(u.url) }, icon('external', 14), 'Profile'),
        h('button', { class: 'btn sm danger', type: 'button', onclick: signOut }, icon('logout', 14), 'Sign out')) : null),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'Projects', h('span', { class: 'right' }, h('button', { class: 'btn sm primary', type: 'button', onclick: () => openModal({ kind: 'addProject' }) }, icon('plus', 14), 'Add a project'))),
      state.projects.length ? state.projects.map(projectRow) : h('div', { class: 'set-row muted' }, 'No projects yet.')),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'Calendars', h('span', { class: 'right' }, h('button', { class: 'btn sm primary', type: 'button', onclick: () => openModal({ kind: 'connectCalendar' }) }, icon('plus', 14), 'Connect a calendar'))),
      state.cal.list.length
        ? state.cal.list.map((c) => h('div', { class: 'set-row', key: 'cal-' + c.id }, icon('calendar', 18),
            h('div', { class: 'grow' }, h('b', null, c.name), h('small', null, 'Read-only, through its private link. The link is kept in Windows Credential Manager.')),
            h('button', { class: 'btn sm danger', type: 'button', onclick: () => openModal({ kind: 'confirm', title: `Disconnect "${c.name}"?`, text: 'Its meetings stop showing in the Agenda and the stored link is deleted from this computer. Nothing changes in Google Calendar.', confirmLabel: 'Disconnect', danger: true, onConfirm: () => removeCalendar(c.id) }) }, 'Disconnect')))
        : h('div', { class: 'set-row' }, h('div', { class: 'grow' }, h('b', null, 'No calendar connected'), h('small', null, 'Connect a Google Calendar by its private link to see your meetings in the Agenda. Read-only; each person connects their own.')))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'Sync'),
      h('div', { class: 'set-row' },
        h('div', { class: 'grow' }, h('b', null, 'Check GitHub for changes'), h('small', null, 'There is no Headway server: the app asks GitHub whether anything changed. When nothing did, the check is free and does not count against GitHub\'s limit.')),
        h('select', { class: 'select', style: { width: '170px' }, 'aria-label': 'How often to check', onchange: (e) => saveSync(Number(e.target.value)) },
          [[15, 'Every 15 seconds'], [30, 'Every 30 seconds'], [60, 'Every minute'], [120, 'Every 2 minutes'], [300, 'Every 5 minutes']].map(([v, label]) => h('option', { value: v, selected: Number(state.settings.syncSeconds) === v }, label))))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'Sign in with a code (for teammates without the GitHub CLI)'),
      h('div', { class: 'set-row' },
        h('div', { class: 'grow' },
          h('b', null, 'GitHub OAuth App Client ID'),
          h('small', null, 'One person registers an OAuth App on GitHub once (Settings, Developer settings, OAuth Apps), turns on "Device flow", and shares its Client ID with the team. It is not a secret.'),
          h('form', { class: 'row', style: { 'margin-top': '8px' }, onsubmit: (e) => { e.preventDefault(); saveClientId(); } },
            h('input', { class: 'input grow', placeholder: 'For example Ov23li…', spellcheck: 'false', autocomplete: 'off', value: local.clientId, oninput: (e) => { local.clientId = e.target.value; } }),
            h('button', { class: 'btn sm', type: 'submit', disabled: local.clientBusy }, local.clientBusy ? spinner() : 'Save'))))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'Claude'),
      h('div', { class: 'set-row' },
        h('div', { class: 'grow' },
          h('b', null, 'Let Claude read and update tasks'),
          h('small', null, 'Headway ships a small MCP server (headway-mcp). Once added to Claude, it can list tasks, change status, priority, due date and assignees, comment, link pull requests and report the project status, using this same sign-in and project list.'),
          mcpCommand
            ? [
                h('div', { class: 'label', style: { margin: '12px 0 6px' } }, 'Claude Code: run this once in a terminal'),
                h('div', { class: 'code-line' }, h('span', null, mcpCommand), h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': 'Copy command', onclick: () => copy(mcpCommand, 'Command') }, icon('copy', 14))),
                h('div', { class: 'label', style: { margin: '12px 0 6px' } }, 'Claude Desktop: add this to claude_desktop_config.json'),
                h('div', { class: 'code-line', style: { 'white-space': 'pre', 'align-items': 'flex-start' } }, h('span', null, mcpJson), h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': 'Copy JSON', onclick: () => copy(mcpJson, 'JSON') }, icon('copy', 14))),
              ]
            : h('small', { style: { 'margin-top': '8px' } }, 'headway-mcp was not found next to the app. Build it with: cargo build --release -p headway-mcp')))),

    h('div', { class: 'panel' },
      h('div', { class: 'panel-head' }, 'About'),
      h('div', { class: 'set-row' }, h('div', { class: 'grow' }, h('b', null, `Headway ${info.version || ''}`), h('small', null, MOCK ? 'Demo data: nothing here is real or saved.' : 'Settings file: ' + (info.configPath || ''))))));
}
