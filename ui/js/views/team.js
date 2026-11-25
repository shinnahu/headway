// Team: who is in the project (the repository's collaborators) and what each is doing.
import { h } from '../dom.js';
import { icon } from '../icons.js';
import { state, openTask, go } from '../store.js';
import { avatar, statusIcon } from '../components.js';
import { openUrl } from '../api.js';
import * as U from '../util.js';

function memberCard(m) {
  const snap = state.snap;
  const mine = snap.tasks.filter((t) => U.isOpen(t) && t.assignees.some((a) => a.login === m.login));
  const doing = mine.filter((t) => t.status !== 'todo');
  const prs = snap.pulls.filter((p) => p.state === 'open' && p.author && p.author.login === m.login).length;
  const isMe = m.login === snap.viewer.login;
  return h('div', { class: 'member', key: m.login },
    avatar(m, 56),
    h('h3', null, U.displayName(m), isMe ? h('span', { class: 'muted', style: { 'font-weight': '500' } }, ' (you)') : null),
    h('div', { class: 'login' }, '@' + m.login),
    h('span', { class: 'pill role' }, U.ROLE_LABEL[m.role] || m.role),
    h('div', { class: 'stats' },
      h('span', null, h('b', null, String(mine.length)), mine.length === 1 ? ' open task' : ' open tasks'),
      h('span', null, h('b', null, String(prs)), prs === 1 ? ' open PR' : ' open PRs')),
    doing.length
      ? h('div', { class: 'doing' }, doing.slice(0, 3).map((t) => h('button', { type: 'button', key: 'd' + t.number, onclick: () => openTask(t.number), 'data-tip': t.title }, statusIcon(t.status, 14), h('span', { class: 'truncate' }, t.title))),
          doing.length > 3 ? h('button', { type: 'button', class: 'muted', onclick: () => { Object.assign(state.tasksUi, { mode: 'list', group: 'active', assignee: m.login, mine: false, priority: null, search: '' }); go('tasks'); } }, `and ${doing.length - 3} more`) : null)
      : h('div', { class: 'idle' }, mine.length ? `Nothing in progress (${mine.length} to do)` : 'Nothing assigned'));
}

export function teamView() {
  const snap = state.snap;
  const leads = snap.members.filter((m) => m.role === 'admin');
  const others = snap.members.filter((m) => m.role !== 'admin');
  const rolesKnown = snap.members.some((m) => m.role !== 'member');
  return h('div', null,
    h('div', { class: 'toolbar' },
      h('span', { class: 'soft' }, `${snap.repo.nameWithOwner} · ${U.plural(snap.members.length, 'member')}`),
      h('span', { class: 'spacer' }),
      h('button', { class: 'btn', type: 'button', 'data-tip': 'Members are the repository\'s collaborators. Invite people on GitHub.', onclick: () => openUrl(snap.repo.url + '/settings/access') }, icon('external', 15), 'Invite on GitHub')),
    h('div', { class: 'org' },
      leads.length ? h('div', { class: 'org-row' }, leads.map(memberCard)) : null,
      leads.length && others.length ? [h('div', { class: 'org-link' }), others.length > 1 ? h('div', { class: 'org-rail' }) : null, h('div', { class: 'org-link' })] : null,
      others.length ? h('div', { class: 'org-row' }, others.map(memberCard)) : null),
    h('div', { class: 'note-card', style: { 'max-width': '760px', margin: '0 auto' } }, icon('info', 17),
      h('span', null, rolesKnown
        ? 'People and roles come from who has access to the repository on GitHub. To add someone or change a role, use the repository\'s access settings there.'
        : 'People come from who can be assigned in the repository. Their roles are only visible to members with write access.')));
}
