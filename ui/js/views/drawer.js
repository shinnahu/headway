// The task panel that slides in from the right: fields, description,
// linked pull requests and branches, comments.
import { h } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, closeTask, updateTask, addComment, deleteComment, saveBody, openMenu, openModal, linkPull, currentProject, gitAction, toast } from '../store.js';
import { avatar, statusPill, priorityPill, pullPill, labelTag, spinner } from '../components.js';
import { statusMenu, priorityMenu, assigneeMenu } from './tasks.js';
import { sanitize } from '../sanitize.js';
import { openUrl, copyText } from '../api.js';
import { t, tn } from '../i18n.js';
import * as U from '../util.js';

function autosize(el) {
  el.style.height = 'auto';
  el.style.height = el.scrollHeight + 'px';
}

function titleField(task) {
  const commit = (el) => {
    const v = el.value.trim().replace(/\s+/g, ' ');
    if (!v) {
      el.value = task.title;
      autosize(el);
    } else if (v !== task.title) updateTask(task.number, { title: v });
  };
  return h('textarea', {
    class: 'd-title', rows: 1, 'aria-label': t('Task title'), spellcheck: 'true', value: task.title, key: 'title-' + task.number,
    oninput: (e) => autosize(e.target),
    onfocus: (e) => autosize(e.target),
    onblur: (e) => commit(e.target),
    onkeydown: (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.target.blur();
      } else if (e.key === 'Escape') {
        e.stopPropagation();
        e.target.value = task.title;
        e.target.blur();
      }
    },
  });
}

function linkPullMenu(anchor, task) {
  const linked = new Set(task.pulls.map((p) => p.number));
  const open = state.snap.pulls.filter((p) => p.state === 'open' && !linked.has(p.number));
  if (!open.length) return void toast(t('There are no other open pull requests to link.'));
  openMenu(anchor, open.map((p) => ({ label: p.title, sub: `#${p.number} · ${p.headRef} · ${U.displayName(p.author)}`, icon: 'pr', onClick: () => linkPull(p.number, task.number, true) })), { search: open.length > 6, minWidth: 320, placeholder: t('Find a pull request…'), alignRight: true });
}

function development(task) {
  const project = currentProject();
  const hasLocal = !!(project && project.localPath);
  const rows = [];
  for (const p of task.pulls) {
    const full = state.snap.pulls.find((x) => x.number === p.number);
    rows.push(h('div', { class: 'dev-row', key: 'pr' + p.number },
      icon(p.state === 'merged' ? 'merge' : 'pr', 16),
      h('span', { class: 'truncate', 'data-tip': p.title }, p.title),
      h('span', { class: 'muted num' }, '#' + p.number),
      h('span', { class: 'right' },
        full && full.checks && full.checks.total ? h('span', { class: 'tag', 'data-tip': t('Checks passed') }, `${full.checks.passed}/${full.checks.total}`) : null,
        pullPill(p),
        h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': t('More'), onclick: (e) => openMenu(e.currentTarget, [
          { label: t('Open on GitHub'), icon: 'external', onClick: () => openUrl(p.url) },
          { label: t('Unlink from this task'), icon: 'x', danger: true, disabled: p.state !== 'open', onClick: () => linkPull(p.number, task.number, false) },
        ], { alignRight: true }) }, icon('more', 16)))));
  }
  for (const b of task.branches) {
    rows.push(h('div', { class: 'dev-row', key: 'br' + b },
      icon('branch', 16),
      h('span', { class: 'truncate', 'data-tip': b }, b),
      h('span', { class: 'right' },
        h('button', { class: 'btn sm quiet', type: 'button', onclick: async () => { if (await copyText(b)) toast(t('Branch name copied')); } }, icon('copy', 14), t('Copy')),
        hasLocal ? h('button', { class: 'btn sm', type: 'button', 'data-tip': t('Fetch, then move your local clone to this branch'), onclick: async () => { await gitAction('fetch', 'git_fetch', {}); gitAction('switch', 'git_switch', { branch: b }, t('Switched to {branch}', { branch: b })); } }, icon('home', 14), t('Switch')) : null)));
  }
  return h('section', { class: 'd-section' },
    h('h3', null, t('Development'),
      h('span', { class: 'right' },
        task.pulls.length === 0 && task.branches.length === 0 && U.isOpen(task) ? h('button', { class: 'btn sm', type: 'button', onclick: () => openModal({ kind: 'startBranch', number: task.number, title: task.title, name: '' }) }, icon('branch', 14), t('Start a branch')) : null,
        h('button', { class: 'btn sm', type: 'button', onclick: (e) => linkPullMenu(e.currentTarget, task) }, icon('link', 14), t('Link a pull request')))),
    rows.length ? rows : h('p', { class: 'hint' }, t('No branch or pull request yet. A pull request whose description says "Closes #{n}" links itself and closes this task when it is merged.', { n: task.number })));
}

function description(d, task) {
  if (d.editBody != null) {
    return h('section', { class: 'd-section' },
      h('h3', null, t('Description')),
      h('div', { class: 'composer' },
        h('textarea', { class: 'textarea', style: { 'min-height': '160px' }, 'aria-label': t('Description (Markdown)'), placeholder: t('Markdown works here.'), value: d.editBody, key: 'body-edit', oninput: (e) => { d.editBody = e.target.value; } }),
        h('div', { class: 'row' },
          h('button', { class: 'btn quiet sm', type: 'button', onclick: () => { d.editBody = null; render(); } }, t('Cancel')),
          h('button', { class: 'btn primary sm', type: 'button', disabled: d.savingBody, onclick: () => saveBody(d.editBody) }, d.savingBody ? spinner() : null, t('Save')))));
  }
  const html = task.bodyHtml ? sanitize(task.bodyHtml) : '';
  return h('section', { class: 'd-section' },
    h('h3', null, t('Description'), h('span', { class: 'right' }, h('button', { class: 'btn sm quiet', type: 'button', onclick: () => { d.editBody = task.body || ''; render(); } }, icon('edit', 14), task.body ? t('Edit') : t('Add')))),
    html ? h('div', { class: 'md', html }) : task.body ? h('div', { class: 'md', style: { 'white-space': 'pre-wrap' } }, task.body) : h('p', { class: 'hint' }, t('No description.')));
}

function comments(d, task) {
  const list = task.comments;
  return h('section', { class: 'd-section' },
    h('h3', null, list ? t('Comments ({n})', { n: list.length }) : t('Comments')),
    list == null
      ? h('div', { class: 'row hint' }, spinner(), t('Loading…'))
      : list.map((c) => h('div', { class: 'comment', key: c.id },
          avatar(c.author, 28),
          h('div', null,
            h('div', { class: 'comment-head' }, h('b', null, U.displayName(c.author)), h('span', { class: 'muted', 'data-tip': U.dateTime(c.createdAt) }, U.ago(c.createdAt)),
              h('span', { class: 'right' },
                h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': t('Open on GitHub'), 'data-tip': t('Open on GitHub'), onclick: () => openUrl(c.url) }, icon('external', 14)),
                c.canDelete ? h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': t('Delete comment'), 'data-tip': t('Delete'), onclick: () => openModal({ kind: 'confirm', title: t('Delete this comment?'), text: t('It is removed from the task on GitHub. This cannot be undone.'), confirmLabel: t('Delete comment'), danger: true, onConfirm: () => deleteComment(c.id) }) }, icon('trash', 14)) : null)),
            h('div', { class: 'md', html: sanitize(c.bodyHtml) })))),
    h('div', { class: 'composer' },
      h('textarea', {
        class: 'textarea', placeholder: t('Write a comment… (Markdown, Ctrl+Enter to send)'), 'aria-label': t('New comment'), value: d.draft, key: 'draft',
        oninput: (e) => { const was = !!d.draft.trim(); d.draft = e.target.value; if (was !== !!d.draft.trim()) render(); },
        onkeydown: (e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && d.draft.trim()) addComment(d.draft); },
      }),
      h('div', { class: 'row' }, h('button', { class: 'btn primary sm', type: 'button', disabled: d.posting || !d.draft.trim(), onclick: () => addComment(d.draft) }, d.posting ? spinner() : icon('comment', 14), t('Comment')))));
}

export function drawerView() {
  const d = state.drawer;
  const task = d.detail;
  const head = h('div', { class: 'drawer-head' },
    h('span', { class: 'muted num', style: { 'font-weight': '600' } }, '#' + d.number),
    d.loading ? spinner() : null,
    h('span', { class: 'grow' }),
    task ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => openUrl(task.url) }, icon('external', 14), t('Open on GitHub')) : null,
    h('button', { class: 'icon-btn ghost', type: 'button', 'aria-label': t('Close'), onclick: closeTask }, icon('x', 18)));

  let body;
  if (!task) {
    body = d.error ? h('div', { class: 'empty' }, icon('alert', 28), h('b', null, t('Could not open this task')), h('span', null, d.error)) : h('div', { class: 'loading', style: { height: '200px' } }, spinner());
  } else {
    const late = task.due && U.isOpen(task) ? -U.daysUntil(task.due) : 0;
    body = [
      titleField(task),
      h('dl', { class: 'props' },
        h('dt', null, t('Status')), h('dd', null, statusPill(task.status, (e) => statusMenu(e.currentTarget, task))),
        h('dt', null, t('Priority')), h('dd', null, priorityPill(task.priority, (e) => priorityMenu(e.currentTarget, task))),
        h('dt', null, t('Assignees')), h('dd', null,
          h('button', { class: 'prop-btn', type: 'button', onclick: (e) => assigneeMenu(e.currentTarget, task) },
            task.assignees.length ? task.assignees.map((a) => h('span', { class: 'row', style: { gap: '6px' } }, avatar(a, 24), U.displayName(a))) : [avatar(null, 24), h('span', { class: 'muted' }, t('Nobody yet'))],
            icon('chevDown', 12))),
        h('dt', null, t('Due date')), h('dd', null,
          h('input', { class: 'input', type: 'date', style: { width: '170px', height: '32px' }, 'aria-label': t('Due date'), value: task.due || '', key: 'due-' + d.number, onchange: (e) => updateTask(task.number, { due: e.target.value || null }) }),
          task.due ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => updateTask(task.number, { due: null }) }, t('Clear')) : null,
          late > 0 ? h('span', { class: 'due over' }, tn(late, 'Overdue by {n} day', 'Overdue by {n} days')) : null),
        task.labels.length ? [h('dt', null, t('Labels')), h('dd', null, task.labels.map(labelTag))] : null,
        h('dt', null, t('Created')), h('dd', { class: 'soft' }, task.author ? `${U.displayName(task.author)}, ` : '', U.ago(task.createdAt), task.closedAt ? ' · ' + t('closed {when}', { when: U.ago(task.closedAt) }) : '')),
      description(d, task),
      development(task),
      comments(d, task),
    ];
  }
  return [
    h('div', { class: 'scrim clear', key: 'drawer-scrim', onclick: closeTask }),
    h('aside', { class: 'drawer', key: 'drawer', role: 'dialog', 'aria-label': task ? t('Task {n}: {title}', { n: d.number, title: task.title }) : t('Task {n}', { n: d.number }) }, head, h('div', { class: 'drawer-body scroll' }, body)),
  ];
}
