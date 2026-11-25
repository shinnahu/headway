// Dialogs: new task, add project, start a branch, new local branch, confirm.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, closeModal, createTask, startBranch, gitAction, openTask, addCalendar, toast } from '../store.js';
import { modalFrame, avatar, spinner, rich } from '../components.js';
import { projectForm, resetProjectForm } from './project-form.js';
import { t, tn } from '../i18n.js';
import * as U from '../util.js';

function newTask(m) {
  if (!m.form) m.form = { title: '', body: '', status: m.status || 'todo', priority: '', due: '', assignees: [], busy: false, error: null };
  const f = m.form;
  const submit = async () => {
    if (!f.title.trim() || f.busy) return;
    f.busy = true;
    f.error = null;
    render();
    try {
      const task = await createTask({ title: f.title.trim(), body: f.body, status: f.status, priority: f.priority || null, due: f.due || null, assignees: f.assignees });
      closeModal();
      if (m.openAfter) openTask(task.number);
    } catch (e) {
      f.busy = false;
      f.error = U.errorText(e);
      render();
    }
  };
  const toggle = (login) => {
    f.assignees = f.assignees.includes(login) ? f.assignees.filter((l) => l !== login) : [...f.assignees, login];
    render();
  };
  return modalFrame({
    title: t('New task'),
    onSubmit: submit,
    body: [
      h('div', { class: 'field' }, h('label', { for: 'nt-title' }, t('Title')),
        h('input', { class: 'input', id: 'nt-title', 'data-autofocus': '', placeholder: t('What needs to be done?'), autocomplete: 'off', value: f.title, oninput: (e) => { const had = !!f.title.trim(); f.title = e.target.value; if (had !== !!f.title.trim()) render(); } })),
      h('div', { class: 'field' }, h('label', { for: 'nt-body' }, t('Description')),
        h('textarea', { class: 'textarea', id: 'nt-body', placeholder: t('Optional. Markdown works here.'), value: f.body, oninput: (e) => { f.body = e.target.value; } })),
      h('div', { class: 'form-grid' },
        h('div', { class: 'field' }, h('label', { for: 'nt-status' }, t('Status')),
          h('select', { class: 'select', id: 'nt-status', onchange: (e) => { f.status = e.target.value; } }, U.STATUSES.slice(0, 3).map((s) => h('option', { value: s, selected: f.status === s }, U.statusLabel(s))))),
        h('div', { class: 'field' }, h('label', { for: 'nt-prio' }, t('Priority')),
          h('select', { class: 'select', id: 'nt-prio', onchange: (e) => { f.priority = e.target.value; } }, h('option', { value: '', selected: !f.priority }, t('No priority')), U.PRIORITIES.map((p) => h('option', { value: p, selected: f.priority === p }, U.priorityLabel(p))))),
        h('div', { class: 'field' }, h('label', { for: 'nt-due' }, t('Due date')),
          h('input', { class: 'input', id: 'nt-due', type: 'date', value: f.due, onchange: (e) => { f.due = e.target.value; } })),
        h('div', { class: 'field' }, h('span', { class: 'label' }, t('Assignees')),
          h('div', { class: 'row', style: { 'flex-wrap': 'wrap', gap: '6px' } },
            (state.snap ? state.snap.members : []).slice(0, 12).map((mem) => h('button', { class: cx('chip', f.assignees.includes(mem.login) && 'on'), type: 'button', key: mem.login, 'aria-pressed': String(f.assignees.includes(mem.login)), onclick: () => toggle(mem.login) }, avatar(mem, 20), U.displayName(mem)))))),
      f.error ? h('p', { class: 'error-text' }, f.error) : null,
    ],
    foot: [
      h('span', { class: 'left hint' }, t('Saved as an issue in {repo}', { repo: state.projectId || t('the repository') })),
      h('button', { class: 'btn quiet', type: 'button', onclick: closeModal }, t('Cancel')),
      h('button', { class: 'btn primary', type: 'submit', disabled: f.busy || !f.title.trim() }, f.busy ? spinner() : null, t('Create task')),
    ],
  });
}

function addProject() {
  return modalFrame({
    title: t('Add a project'),
    wide: true,
    body: [
      h('p', { class: 'soft' }, t('Any repository you can reach on GitHub works, new or existing. Its issues become tasks; adding it changes nothing in the repository.')),
      projectForm({ submitLabel: t('Add project'), onCancel: () => { resetProjectForm(); closeModal(); } }),
    ],
    foot: null,
  });
}

function startBranchModal(m) {
  const slug = `${m.number}-${(m.title || 'task').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 40).replace(/-+$/, '')}`;
  if (m.name === '') m.name = slug;
  const submit = async () => {
    if (m.busy) return;
    m.busy = true;
    render();
    const r = await startBranch(m.number, m.name.trim());
    m.busy = false;
    if (r) closeModal();
    else render();
  };
  return modalFrame({
    title: t('Start a branch for #{n}', { n: m.number }),
    onSubmit: submit,
    body: [
      h('p', { class: 'soft' }, t('Headway creates this branch on GitHub from the default branch and links it to the task. A pull request opened from it attaches to the task by itself.')),
      h('div', { class: 'field' }, h('label', { for: 'sb-name' }, t('Branch name')),
        h('input', { class: 'input', id: 'sb-name', 'data-autofocus': '', spellcheck: 'false', autocomplete: 'off', value: m.name, oninput: (e) => { m.name = e.target.value; } })),
    ],
    foot: [
      h('button', { class: 'btn quiet', type: 'button', onclick: closeModal }, t('Cancel')),
      h('button', { class: 'btn primary', type: 'submit', disabled: m.busy }, m.busy ? spinner() : icon('branch', 15), t('Create branch')),
    ],
  });
}

function newBranchModal(m) {
  if (m.name == null) m.name = '';
  if (m.switchTo == null) m.switchTo = true;
  const submit = async () => {
    if (!m.name.trim() || m.busy) return;
    m.busy = true;
    render();
    await gitAction('branch', 'git_create_branch', { branch: m.name.trim(), from: m.from, switch: m.switchTo }, t('Branch {name} created', { name: m.name.trim() }));
    closeModal();
  };
  return modalFrame({
    title: t('New branch from here'),
    onSubmit: submit,
    body: [
      h('p', { class: 'soft' }, rich('Creates a local branch starting at **{from}**. Nothing is pushed.', { from: m.fromLabel || m.from })),
      h('div', { class: 'field' }, h('label', { for: 'nb-name' }, t('Branch name')),
        h('input', { class: 'input', id: 'nb-name', 'data-autofocus': '', spellcheck: 'false', autocomplete: 'off', placeholder: 'feature/short-name', value: m.name, oninput: (e) => { const had = !!m.name.trim(); m.name = e.target.value; if (had !== !!m.name.trim()) render(); } })),
      h('label', { class: 'row' }, h('input', { type: 'checkbox', checked: m.switchTo, onchange: (e) => { m.switchTo = e.target.checked; } }), t('Switch to it right away')),
    ],
    foot: [
      h('button', { class: 'btn quiet', type: 'button', onclick: closeModal }, t('Cancel')),
      h('button', { class: 'btn primary', type: 'submit', disabled: m.busy || !m.name.trim() }, m.busy ? spinner() : null, t('Create branch')),
    ],
  });
}

function connectCalendar(m) {
  if (!m.form) m.form = { url: '', name: '', show: false, busy: false, error: null };
  const f = m.form;
  const submit = async () => {
    if (!f.url.trim() || f.busy) return;
    f.busy = true;
    f.error = null;
    render();
    try {
      const r = await addCalendar(f.url.trim(), f.name.trim());
      toast(tn(r.events, '"{name}" connected ({n} event in it)', '"{name}" connected ({n} events in it)', { name: r.calendar.name }));
      closeModal();
    } catch (e) {
      f.busy = false;
      f.error = U.errorText(e);
      render();
    }
  };
  return modalFrame({
    title: t('Connect a Google Calendar'),
    onSubmit: submit,
    body: [
      // The bold words are the names Google Calendar itself shows.
      h('ol', { class: 'steps' },
        h('li', null, rich('Open Google Calendar in a browser on a computer and go to **Settings**.')),
        h('li', null, t('On the left, click the calendar you want.')),
        h('li', null, rich('Under **Integrate calendar**, copy the **Secret address in iCal format**.')),
        h('li', null, t('Paste it below.'))),
      h('div', { class: 'field' }, h('label', { for: 'cc-url' }, t('Secret address')),
        h('div', { class: 'row' },
          h('input', { class: 'input grow', id: 'cc-url', 'data-autofocus': '', type: f.show ? 'text' : 'password', autocomplete: 'off', spellcheck: 'false', placeholder: 'https://calendar.google.com/calendar/ical/…/basic.ics', value: f.url, oninput: (e) => { const had = !!f.url.trim(); f.url = e.target.value; if (had !== !!f.url.trim()) render(); } }),
          h('button', { class: 'btn', type: 'button', onclick: () => { f.show = !f.show; render(); } }, icon('eye', 15), f.show ? t('Hide') : t('Show')))),
      h('div', { class: 'field' }, h('label', { for: 'cc-name' }, t('Name (optional)')),
        h('input', { class: 'input', id: 'cc-name', autocomplete: 'off', placeholder: t('For example: My calendar'), value: f.name, oninput: (e) => { f.name = e.target.value; } })),
      h('p', { class: 'hint' }, t('Read-only: Headway can show your events but cannot change them. The address works like a password, so it is kept in Windows Credential Manager on this computer and nowhere else. Each teammate connects their own.')),
      f.error ? h('p', { class: 'error-text' }, f.error) : null,
    ],
    foot: [
      h('button', { class: 'btn quiet', type: 'button', onclick: closeModal }, t('Cancel')),
      h('button', { class: 'btn primary', type: 'submit', disabled: f.busy || !f.url.trim() }, f.busy ? spinner() : null, t('Connect')),
    ],
  });
}

function confirmModal(m) {
  const go = async () => {
    closeModal();
    if (m.onConfirm) await m.onConfirm();
  };
  return modalFrame({
    title: m.title,
    onSubmit: go,
    body: [h('p', { class: 'soft', style: { 'white-space': 'pre-line' } }, m.text)],
    foot: [
      h('button', { class: 'btn quiet', type: 'button', onclick: closeModal }, t('Cancel')),
      h('button', { class: cx('btn', m.danger ? 'danger' : 'primary'), type: 'submit', 'data-autofocus': '' }, m.confirmLabel || t('OK')),
    ],
  });
}

export function modalView() {
  const m = state.modal;
  switch (m.kind) {
    case 'newTask': return newTask(m);
    case 'addProject': return addProject();
    case 'startBranch': return startBranchModal(m);
    case 'newBranch': return newBranchModal(m);
    case 'connectCalendar': return connectCalendar(m);
    case 'confirm': return confirmModal(m);
    default: return h('div', null);
  }
}

/** Puts the caret in the first field of a dialog that just opened. */
export function focusModal() {
  const m = state.modal;
  if (!m || m.focused) return;
  const el = document.querySelector('#layer .modal [data-autofocus]');
  if (el) {
    m.focused = true;
    el.focus();
  }
}
