// Team: who is in the project (the repository's collaborators) and what each is doing.
import { h } from '../dom.js';
import { icon } from '../icons.js';
import { state, openTask, go } from '../store.js';
import { avatar, statusIcon } from '../components.js';
import { openUrl } from '../api.js';
import { t, tn } from '../i18n.js';
import * as U from '../util.js';

function memberCard(m) {
  const snap = state.snap;
  const mine = snap.tasks.filter((x) => U.isOpen(x) && x.assignees.some((a) => a.login === m.login));
  const doing = mine.filter((x) => x.status !== 'todo');
  const prs = snap.pulls.filter((p) => p.state === 'open' && p.author && p.author.login === m.login).length;
  const isMe = m.login === snap.viewer.login;
  return h('div', { class: 'member', key: m.login },
    avatar(m, 56),
    h('h3', null, U.displayName(m), isMe ? h('span', { class: 'muted', style: { 'font-weight': '500' } }, ' ' + t('(you)')) : null),
    h('div', { class: 'login' }, '@' + m.login),
    h('span', { class: 'pill role' }, U.roleLabel(m.role)),
    h('div', { class: 'stats' },
      h('span', null, h('b', null, String(mine.length)), ' ' + (mine.length === 1 ? t('open task') : t('open tasks'))),
      h('span', null, h('b', null, String(prs)), ' ' + (prs === 1 ? t('open PR') : t('open PRs')))),
    doing.length
      ? h('div', { class: 'doing' }, doing.slice(0, 3).map((x) => h('button', { type: 'button', key: 'd' + x.number, onclick: () => openTask(x.number), 'data-tip': x.title }, statusIcon(x.status, 14), h('span', { class: 'truncate' }, x.title))),
          doing.length > 3 ? h('button', { type: 'button', class: 'muted', onclick: () => { Object.assign(state.tasksUi, { mode: 'list', group: 'active', assignee: m.login, mine: false, priority: null, search: '' }); go('tasks'); } }, t('and {n} more', { n: doing.length - 3 })) : null)
      : h('div', { class: 'idle' }, mine.length ? t('Nothing in progress ({n} to do)', { n: mine.length }) : t('Nothing assigned')));
}

export function teamView() {
  const snap = state.snap;
  const leads = snap.members.filter((m) => m.role === 'admin');
  const others = snap.members.filter((m) => m.role !== 'admin');
  const rolesKnown = snap.members.some((m) => m.role !== 'member');
  return h('div', null,
    h('div', { class: 'toolbar' },
      h('span', { class: 'soft' }, `${snap.repo.nameWithOwner} · ${tn(snap.members.length, '{n} member', '{n} members')}`),
      h('span', { class: 'spacer' }),
      h('button', { class: 'btn', type: 'button', 'data-tip': t('Members are the repository\'s collaborators. Invite people on GitHub.'), onclick: () => openUrl(snap.repo.url + '/settings/access') }, icon('external', 15), t('Invite on GitHub'))),
    h('div', { class: 'org' },
      leads.length ? h('div', { class: 'org-row' }, leads.map(memberCard)) : null,
      leads.length && others.length ? [h('div', { class: 'org-link' }), others.length > 1 ? h('div', { class: 'org-rail' }) : null, h('div', { class: 'org-link' })] : null,
      others.length ? h('div', { class: 'org-row' }, others.map(memberCard)) : null),
    h('div', { class: 'note-card', style: { 'max-width': '760px', margin: '0 auto' } }, icon('info', 17),
      h('span', null, rolesKnown
        ? t('People and roles come from who has access to the repository on GitHub. To add someone or change a role, use the repository\'s access settings there.')
        : t('People come from who can be assigned in the repository. Their roles are only visible to members with write access.'))));
}
