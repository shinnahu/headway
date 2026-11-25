// "Add a project": choose a repository you can reach on GitHub and, optionally,
// the folder of your local clone (needed for the Branches screen).
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, addProject, closeModal, go } from '../store.js';
import { spinner } from '../components.js';
import { api, pickFolder, HAS_FOLDER_PICKER } from '../api.js';
import { t } from '../i18n.js';
import * as U from '../util.js';

const form = { repos: null, loading: false, loadError: null, query: '', repo: '', path: '', busy: false, error: null };

export function resetProjectForm() {
  Object.assign(form, { query: '', repo: '', path: '', busy: false, error: null });
}

async function loadRepos() {
  if (form.loading || form.repos) return;
  form.loading = true;
  try {
    form.repos = await api('repos_list');
  } catch (e) {
    form.loadError = U.errorText(e);
    form.repos = [];
  }
  form.loading = false;
  render();
}

async function submit() {
  const repo = (form.repo || form.query).trim();
  if (!repo || form.busy) return;
  form.busy = true;
  form.error = null;
  render();
  try {
    await addProject(repo, form.path.trim());
    resetProjectForm();
    closeModal();
    go('health');
  } catch (e) {
    form.error = U.errorText(e);
  }
  form.busy = false;
  render();
}

async function browse() {
  const picked = await pickFolder();
  if (picked) {
    form.path = picked;
    render();
  }
}

export function projectForm({ submitLabel = t('Add project'), onCancel = null } = {}) {
  loadRepos();
  const q = form.query.trim().toLowerCase();
  const already = new Set(state.projects.map((p) => p.id.toLowerCase()));
  const list = (form.repos || []).filter((r) => !r.isArchived && (!q || r.nameWithOwner.toLowerCase().includes(q))).slice(0, 40);
  return h('div', { class: 'col', style: { gap: '14px' } },
    h('div', { class: 'field' },
      h('label', { for: 'pf-repo' }, t('Repository')),
      h('input', { class: 'input', id: 'pf-repo', placeholder: t('Search your repositories, or type owner/name'), autocomplete: 'off', spellcheck: 'false', value: form.query, oninput: (e) => { form.query = e.target.value; form.repo = ''; render(); } }),
      form.loading
        ? h('div', { class: 'row hint' }, spinner(), t('Loading your repositories…'))
        : list.length
          ? h('div', { class: 'repo-list scroll' }, list.map((r) => {
              const added = already.has(r.nameWithOwner.toLowerCase());
              return h('button', { class: cx('repo-opt', form.repo === r.nameWithOwner && 'on'), type: 'button', key: r.nameWithOwner, onclick: () => { form.repo = r.nameWithOwner; form.query = r.nameWithOwner; render(); } },
                icon(r.isPrivate ? 'lock' : 'repo', 16),
                h('span', { class: 'grow' }, h('b', null, r.nameWithOwner), r.description ? h('small', { class: 'truncate' }, r.description) : null),
                added ? h('span', { class: 'tag' }, t('Added')) : r.pushedAt ? h('small', { class: 'muted', style: { 'white-space': 'nowrap' } }, U.ago(r.pushedAt)) : null);
            }))
          : h('div', { class: 'hint' }, form.loadError || (q ? t('No repository of yours matches. If it belongs to someone else, type it as owner/name.') : t('No repositories found for this account.')))),
    h('div', { class: 'field' },
      h('label', { for: 'pf-path' }, t('Local folder (optional)')),
      h('div', { class: 'row' },
        h('input', { class: 'input grow', id: 'pf-path', placeholder: t('Where you cloned it, for example C:\\Work\\my-repo'), autocomplete: 'off', spellcheck: 'false', value: form.path, oninput: (e) => { form.path = e.target.value; } }),
        HAS_FOLDER_PICKER ? h('button', { class: 'btn', type: 'button', onclick: browse }, icon('folder', 16), t('Browse…')) : null),
      h('span', { class: 'hint' }, t('Only needed for the Branches screen. You can add it later in Settings.'))),
    form.error ? h('p', { class: 'error-text' }, form.error) : null,
    h('div', { class: 'row', style: { 'justify-content': 'flex-end' } },
      onCancel ? h('button', { class: 'btn quiet', type: 'button', onclick: onCancel }, t('Cancel')) : null,
      h('button', { class: 'btn primary', type: 'button', disabled: form.busy || !(form.repo || form.query).trim(), onclick: submit }, form.busy ? spinner() : icon('plus', 16), submitLabel)));
}
