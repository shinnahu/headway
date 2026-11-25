// The task panel that slides in from the right: fields, description,
// linked pull requests and branches, comments.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, render, closeTask, updateTask, addComment, deleteComment, saveBody, openMenu, openModal, linkPull, currentProject, gitAction, toast } from '../store.js';
import { avatar, statusPill, priorityPill, pullPill, labelTag, spinner } from '../components.js';
import { statusMenu, priorityMenu, assigneeMenu } from './tasks.js';
import { sanitize } from '../sanitize.js';
import { openUrl, copyText } from '../api.js';
import * as U from '../util.js';

function autosize(el) {
  el.style.height = 'auto';
  el.style.height = el.scrollHeight + 'px';
}

function titleField(t) {
  const commit = (el) => {
    const v = el.value.trim().replace(/\s+/g, ' ');
    if (!v) {
      el.value = t.title;
      autosize(el);
    } else if (v !== t.title) updateTask(t.number, { title: v });
  };
  return h('textarea', {
    class: 'd-title', rows: 1, 'aria-label': 'Task title', spellcheck: 'true', value: t.title, key: 'title-' + t.number,
    oninput: (e) => autosize(e.target),
    onfocus: (e) => autosize(e.target),
    onblur: (e) => commit(e.target),
    onkeydown: (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.target.blur();
      } else if (e.key === 'Escape') {
        e.stopPropagation();
        e.target.value = t.title;
        e.target.blur();
      }
    },
  });
}

function linkPullMenu(anchor, t) {
  const linked = new Set(t.pulls.map((p) => p.number));
  const open = state.snap.pulls.filter((p) => p.state === 'open' && !linked.has(p.number));
  if (!open.length) return void toast('There are no other open pull requests to link.');
  openMenu(anchor, open.map((p) => ({ label: p.title, sub: `#${p.number} · ${p.headRef} · ${U.displayName(p.author)}`, icon: 'pr', onClick: () => linkPull(p.number, t.number, true) })), { search: open.length > 6, minWidth: 320, placeholder: 'Find a pull request…', alignRight: true });
}

function development(t) {
  const project = currentProject();
  const hasLocal = !!(project && project.localPath);
  const rows = [];
  for (const p of t.pulls) {
    const full = state.snap.pulls.find((x) => x.number === p.number);
    rows.push(h('div', { class: 'dev-row', key: 'pr' + p.number },
      icon(p.state === 'merged' ? 'merge' : 'pr', 16),
      h('span', { class: 'truncate', 'data-tip': p.title }, p.title),
      h('span', { class: 'muted num' }, '#' + p.number),
      h('span', { class: 'right' },
        full && full.checks && full.checks.total ? h('span', { class: cx('tag'), 'data-tip': 'Checks passed' }, `${full.checks.passed}/${full.checks.total}`) : null,
        pullPill(p),
        h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': 'More', onclick: (e) => openMenu(e.currentTarget, [
          { label: 'Open on GitHub', icon: 'external', onClick: () => openUrl(p.url) },
          { label: 'Unlink from this task', icon: 'x', danger: true, disabled: p.state !== 'open', onClick: () => linkPull(p.number, t.number, false) },
        ], { alignRight: true }) }, icon('more', 16)))));
  }
  for (const b of t.branches) {
    rows.push(h('div', { class: 'dev-row', key: 'br' + b },
      icon('branch', 16),
      h('span', { class: 'truncate', 'data-tip': b }, b),
      h('span', { class: 'right' },
        h('button', { class: 'btn sm quiet', type: 'button', onclick: async () => { if (await copyText(b)) toast('Branch name copied'); } }, icon('copy', 14), 'Copy'),
        hasLocal ? h('button', { class: 'btn sm', type: 'button', 'data-tip': 'Fetch, then move your local clone to this branch', onclick: async () => { await gitAction('fetch', 'git_fetch', {}); gitAction('switch', 'git_switch', { branch: b }, `Switched to ${b}`); } }, icon('home', 14), 'Switch') : null)));
  }
  return h('section', { class: 'd-section' },
    h('h3', null, 'Development',
      h('span', { class: 'right' },
        t.pulls.length === 0 && t.branches.length === 0 && U.isOpen(t) ? h('button', { class: 'btn sm', type: 'button', onclick: () => openModal({ kind: 'startBranch', number: t.number, title: t.title, name: '' }) }, icon('branch', 14), 'Start a branch') : null,
        h('button', { class: 'btn sm', type: 'button', onclick: (e) => linkPullMenu(e.currentTarget, t) }, icon('link', 14), 'Link a pull request'))),
    rows.length ? rows : h('p', { class: 'hint' }, 'No branch or pull request yet. A pull request whose description says "Closes #' + t.number + '" links itself and closes this task when it is merged.'));
}

function description(d, t) {
  if (d.editBody != null) {
    return h('section', { class: 'd-section' },
      h('h3', null, 'Description'),
      h('div', { class: 'composer' },
        h('textarea', { class: 'textarea', style: { 'min-height': '160px' }, 'aria-label': 'Description (Markdown)', placeholder: 'Markdown works here.', value: d.editBody, key: 'body-edit', oninput: (e) => { d.editBody = e.target.value; } }),
        h('div', { class: 'row' },
          h('button', { class: 'btn quiet sm', type: 'button', onclick: () => { d.editBody = null; render(); } }, 'Cancel'),
          h('button', { class: 'btn primary sm', type: 'button', disabled: d.savingBody, onclick: () => saveBody(d.editBody) }, d.savingBody ? spinner() : null, 'Save'))));
  }
  const html = t.bodyHtml ? sanitize(t.bodyHtml) : '';
  return h('section', { class: 'd-section' },
    h('h3', null, 'Description', h('span', { class: 'right' }, h('button', { class: 'btn sm quiet', type: 'button', onclick: () => { d.editBody = t.body || ''; render(); } }, icon('edit', 14), t.body ? 'Edit' : 'Add'))),
    html ? h('div', { class: 'md', html }) : t.body ? h('div', { class: 'md', style: { 'white-space': 'pre-wrap' } }, t.body) : h('p', { class: 'hint' }, 'No description.'));
}

function comments(d, t) {
  const list = t.comments;
  return h('section', { class: 'd-section' },
    h('h3', null, list ? `Comments (${list.length})` : 'Comments'),
    list == null
      ? h('div', { class: 'row hint' }, spinner(), 'Loading…')
      : list.map((c) => h('div', { class: 'comment', key: c.id },
          avatar(c.author, 28),
          h('div', null,
            h('div', { class: 'comment-head' }, h('b', null, U.displayName(c.author)), h('span', { class: 'muted', 'data-tip': new Date(c.createdAt).toLocaleString() }, U.ago(c.createdAt)),
              h('span', { class: 'right' },
                h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': 'Open on GitHub', 'data-tip': 'Open on GitHub', onclick: () => openUrl(c.url) }, icon('external', 14)),
                c.canDelete ? h('button', { class: 'icon-btn ghost sm', type: 'button', 'aria-label': 'Delete comment', 'data-tip': 'Delete', onclick: () => openModal({ kind: 'confirm', title: 'Delete this comment?', text: 'It is removed from the task on GitHub. This cannot be undone.', confirmLabel: 'Delete comment', danger: true, onConfirm: () => deleteComment(c.id) }) }, icon('trash', 14)) : null)),
            h('div', { class: 'md', html: sanitize(c.bodyHtml) })))),
    h('div', { class: 'composer' },
      h('textarea', {
        class: 'textarea', placeholder: 'Write a comment… (Markdown, Ctrl+Enter to send)', 'aria-label': 'New comment', value: d.draft, key: 'draft',
        oninput: (e) => { const was = !!d.draft.trim(); d.draft = e.target.value; if (was !== !!d.draft.trim()) render(); },
        onkeydown: (e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && d.draft.trim()) addComment(d.draft); },
      }),
      h('div', { class: 'row' }, h('button', { class: 'btn primary sm', type: 'button', disabled: d.posting || !d.draft.trim(), onclick: () => addComment(d.draft) }, d.posting ? spinner() : icon('comment', 14), 'Comment'))));
}

export function drawerView() {
  const d = state.drawer;
  const t = d.detail;
  const head = h('div', { class: 'drawer-head' },
    h('span', { class: 'muted num', style: { 'font-weight': '600' } }, '#' + d.number),
    d.loading ? spinner() : null,
    h('span', { class: 'grow' }),
    t ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => openUrl(t.url) }, icon('external', 14), 'Open on GitHub') : null,
    h('button', { class: 'icon-btn ghost', type: 'button', 'aria-label': 'Close', onclick: closeTask }, icon('x', 18)));

  let body;
  if (!t) {
    body = d.error ? h('div', { class: 'empty' }, icon('alert', 28), h('b', null, 'Could not open this task'), h('span', null, d.error)) : h('div', { class: 'loading', style: { height: '200px' } }, spinner());
  } else {
    body = [
      titleField(t),
      h('dl', { class: 'props' },
        h('dt', null, 'Status'), h('dd', null, statusPill(t.status, (e) => statusMenu(e.currentTarget, t))),
        h('dt', null, 'Priority'), h('dd', null, priorityPill(t.priority, (e) => priorityMenu(e.currentTarget, t))),
        h('dt', null, 'Assignees'), h('dd', null,
          h('button', { class: 'prop-btn', type: 'button', onclick: (e) => assigneeMenu(e.currentTarget, t) },
            t.assignees.length ? t.assignees.map((a) => h('span', { class: 'row', style: { gap: '6px' } }, avatar(a, 24), U.displayName(a))) : [avatar(null, 24), h('span', { class: 'muted' }, 'Nobody yet')],
            icon('chevDown', 12))),
        h('dt', null, 'Due date'), h('dd', null,
          h('input', { class: 'input', type: 'date', style: { width: '170px', height: '32px' }, 'aria-label': 'Due date', value: t.due || '', key: 'due-' + d.number, onchange: (e) => updateTask(t.number, { due: e.target.value || null }) }),
          t.due ? h('button', { class: 'btn sm quiet', type: 'button', onclick: () => updateTask(t.number, { due: null }) }, 'Clear') : null,
          t.due && U.isOpen(t) && U.daysUntil(t.due) < 0 ? h('span', { class: 'due over' }, `Overdue by ${U.plural(-U.daysUntil(t.due), 'day')}`) : null),
        t.labels.length ? [h('dt', null, 'Labels'), h('dd', null, t.labels.map(labelTag))] : null,
        h('dt', null, 'Created'), h('dd', { class: 'soft' }, t.author ? `${U.displayName(t.author)}, ` : '', U.ago(t.createdAt), t.closedAt ? ` · closed ${U.ago(t.closedAt)}` : '')),
      description(d, t),
      development(t),
      comments(d, t),
    ];
  }
  return [
    h('div', { class: 'scrim clear', key: 'drawer-scrim', onclick: closeTask }),
    h('aside', { class: 'drawer', key: 'drawer', role: 'dialog', 'aria-label': t ? `Task ${d.number}: ${t.title}` : `Task ${d.number}` }, head, h('div', { class: 'drawer-body scroll' }, body)),
  ];
}
