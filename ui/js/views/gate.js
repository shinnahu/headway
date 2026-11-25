// What you see before the app proper: sign in to GitHub, then add a project.
import { h, cx } from '../dom.js';
import { icon } from '../icons.js';
import { state, signInWithGh, signInWithToken, signInWithDevice, cancelDevice, signOut, changeLanguage } from '../store.js';
import { spinner } from '../components.js';
import { openUrl, copyText } from '../api.js';
import { projectForm } from './project-form.js';
import { t, LANGUAGES, language } from '../i18n.js';
import * as U from '../util.js';

function brand() {
  return h('div', { class: 'brand' }, h('img', { src: 'assets/logo.svg', alt: '' }), h('b', null, 'Headway'));
}

/** English / Español, reachable before there is a Settings screen to open. */
function languageSwitch() {
  return h('div', { class: 'gate-lang' }, icon('globe', 14),
    LANGUAGES.map(([code, name]) => h('button', { class: cx(language() === code && 'on'), type: 'button', key: code, lang: code, 'aria-pressed': String(language() === code), onclick: () => changeLanguage(code) }, name)));
}

function deviceStep(d) {
  return h('div', { class: 'gate-card' },
    brand(),
    h('h1', null, t('Enter this code on GitHub')),
    h('p', { class: 'lead' }, t('Open GitHub, type the code, and approve Headway. This window continues by itself.')),
    h('div', { class: 'code-box' }, d.userCode),
    h('div', { class: 'row', style: { 'margin-top': '14px' } },
      h('button', { class: 'btn primary grow', type: 'button', onclick: () => openUrl(d.verificationUri) }, icon('external', 16), t('Open github.com/login/device')),
      h('button', { class: 'btn', type: 'button', onclick: () => copyText(d.userCode) }, icon('copy', 16), t('Copy code'))),
    h('div', { class: 'row', style: { 'margin-top': '16px', color: 'var(--ink-3)' } }, spinner(), h('span', { class: 'grow' }, t('Waiting for GitHub…')), h('button', { class: 'btn quiet sm', type: 'button', onclick: cancelDevice }, t('Cancel'))));
}

function signIn() {
  const g = state.gate;
  const auth = state.auth || { ghAccounts: [] };
  if (g.device) return deviceStep(g.device);
  const accounts = auth.ghAccounts || [];
  const busy = !!g.busy;
  return h('div', { class: 'gate-card' },
    brand(),
    h('h1', null, t('Sign in with GitHub')),
    h('p', { class: 'lead' }, t('Headway keeps your team\'s tasks as issues in your own GitHub repository. Your GitHub account is the only account there is.')),
    h('div', { class: 'gate-list' },
      accounts.map((a) => h('button', { class: 'choice', type: 'button', disabled: busy, onclick: () => signInWithGh(a.login) },
        h('span', { class: 'avatar s32' }, icon('terminal', 17)),
        h('span', { class: 'grow' }, h('b', null, t('Continue as {login}', { login: a.login })), h('small', null, t('Uses the GitHub CLI session already on this computer'))),
        g.busy === 'gh:' + a.login ? spinner() : icon('chevRight', 17, { class: 'chev' }))),
      h('button', { class: 'choice', type: 'button', disabled: busy || !auth.deviceFlowConfigured, onclick: signInWithDevice },
        h('span', { class: 'avatar s32' }, icon('key', 17)),
        h('span', { class: 'grow' }, h('b', null, t('Sign in with a code')), h('small', null, auth.deviceFlowConfigured ? t('GitHub shows a short code to confirm') : t('Needs a one-time setup by whoever runs the team: see the README, "Sign in"'))),
        g.busy === 'device' ? spinner() : icon('chevRight', 17, { class: 'chev' }))),
    h('div', { class: 'or' }, t('or paste a token')),
    h('form', { class: 'row', onsubmit: (e) => { e.preventDefault(); if (g.token.trim()) signInWithToken(g.token.trim()); } },
      h('input', { class: 'input grow', type: 'password', autocomplete: 'off', placeholder: t('Personal access token (needs the repo scope)'), 'aria-label': t('Personal access token'), value: g.token, oninput: (e) => { g.token = e.target.value; } }),
      h('button', { class: 'btn', type: 'submit', disabled: busy }, g.busy === 'token' ? spinner() : t('Sign in'))),
    h('p', { class: 'hint', style: { 'margin-top': '8px' } }, t('The token is stored in Windows Credential Manager, not in a file.')),
    g.error ? h('p', { class: 'error-text', style: { 'margin-top': '12px' } }, g.error) : null);
}

function firstProject() {
  const u = state.user;
  return h('div', { class: 'gate-card wide' },
    brand(),
    h('h1', null, u ? t('Welcome, {name}', { name: U.displayName(u) }) : t('Welcome')),
    h('p', { class: 'lead' }, t('Pick the repository your team works in. Its issues become your tasks; nothing in the repository is changed by adding it.')),
    projectForm({ submitLabel: t('Add project') }),
    h('div', { class: 'row', style: { 'margin-top': '14px', 'justify-content': 'center' } }, h('button', { class: 'btn quiet sm', type: 'button', onclick: signOut }, t('Use a different account'))));
}

export function gateView() {
  return h('div', { class: 'gate', key: 'gate-' + state.phase }, state.phase === 'signin' ? signIn() : firstProject(), languageSwitch());
}
