// Checks the Spanish translation against the code.
//
//   node scripts/check-i18n.mjs          report problems, exit 1 if any
//   node scripts/check-i18n.mjs --list   also print every text the UI uses
//
// What it checks:
//   1. every text the UI passes to t(), tn(), N() or rich() has a Spanish version;
//   2. a translation keeps the same {placeholders} and **bold** marks;
//   3. no Spanish entry is left over from text that no longer exists;
//   4. every message the Rust core can send has a Spanish version, except the
//      ones only a program (the MCP client) can cause.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const uiDir = join(root, 'ui', 'js');
const coreDir = join(root, 'crates');

const walk = (dir, ext, skip = () => false) =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (skip(path)) return [];
    return statSync(path).isDirectory() ? walk(path, ext, skip) : path.endsWith(ext) ? [path] : [];
  });

// ---- 1. texts used by the UI -------------------------------------------------------

/** Reads a JS string literal starting at `i` (a quote). Returns [value, endIndex] or null. */
function readString(src, i) {
  const quote = src[i];
  if (quote !== "'" && quote !== '"') return null;
  let j = i + 1;
  let raw = '';
  while (j < src.length && src[j] !== quote) {
    if (src[j] === '\n') return null;
    if (src[j] === '\\') {
      raw += src[j] + src[j + 1];
      j += 2;
    } else {
      raw += src[j];
      j += 1;
    }
  }
  if (src[j] !== quote) return null;
  const value = raw.replace(/\\(u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|x[0-9a-fA-F]{2}|[\s\S])/g, (m, e) => {
    if (e[0] === 'u') return String.fromCodePoint(parseInt(e.replace(/[u{}]/g, ''), 16));
    if (e[0] === 'x') return String.fromCharCode(parseInt(e.slice(1), 16));
    return { n: '\n', t: '\t', r: '\r', 0: '\0' }[e] ?? e;
  });
  return [value, j + 1];
}

const skipSpace = (src, i) => {
  while (i < src.length && /\s/.test(src[i])) i += 1;
  return i;
};

/** From just after "tn(", the index after the first top-level comma. */
function afterFirstArgument(src, i) {
  let depth = 0;
  while (i < src.length) {
    const ch = src[i];
    if (ch === "'" || ch === '"' || ch === '`') {
      const end = src.indexOf(ch, i + 1);
      i = end < 0 ? src.length : end + 1;
      continue;
    }
    if ('([{'.includes(ch)) depth += 1;
    else if (')]}'.includes(ch)) {
      if (depth === 0) return -1;
      depth -= 1;
    } else if (ch === ',' && depth === 0) return i + 1;
    i += 1;
  }
  return -1;
}

const used = new Map(); // text -> first place it is used
const problems = [];

for (const file of walk(uiDir, '.js', (p) => p === join(uiDir, 'i18n'))) {
  const src = readFileSync(file, 'utf8');
  const where = (index) => `${relative(root, file).replaceAll('\\', '/')}:${src.slice(0, index).split('\n').length}`;
  const note = (text, index) => {
    if (!used.has(text)) used.set(text, where(index));
  };
  // Calls only: "function tn(" is where it is defined.
  for (const m of src.matchAll(/(?<![\w.$])(?<!function )(t|N|rich|tn)\(/g)) {
    let i = skipSpace(src, m.index + m[0].length);
    if (m[1] === 'tn') {
      i = afterFirstArgument(src, i);
      if (i < 0) continue;
      for (let k = 0; k < 2; k++) {
        i = skipSpace(src, i);
        const s = readString(src, i);
        if (!s) {
          problems.push(`${where(m.index)}: tn() needs its two texts written out as plain strings`);
          break;
        }
        note(s[0], m.index);
        i = skipSpace(src, s[1]);
        if (src[i] === ',') i += 1;
      }
      continue;
    }
    if (src[i] === '`') {
      problems.push(`${where(m.index)}: ${m[1]}() is given a template string; use a plain string with {placeholders}`);
      continue;
    }
    const s = readString(src, i);
    if (s) note(s[0], m.index); // anything else is a variable holding a text marked with N()
  }
}

// ---- 2 and 3. the Spanish dictionary -------------------------------------------------

const { ES, ES_MESSAGES, ES_ERRORS } = await import(pathToFileURL(join(uiDir, 'i18n', 'es.js')).href);

const placeholders = (text) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',');
const bolds = (text) => (text.match(/\*\*/g) || []).length;

const missing = [];
for (const [text, place] of used) {
  const es = ES[text];
  if (es == null) {
    missing.push([text, place]);
    continue;
  }
  if (typeof es !== 'string' || !es.trim()) problems.push(`${place}: the Spanish text for ${JSON.stringify(text)} is empty`);
  else {
    if (placeholders(text) !== placeholders(es)) problems.push(`${place}: placeholders differ\n    en: ${text}\n    es: ${es}`);
    if (bolds(text) !== bolds(es) || bolds(es) % 2) problems.push(`${place}: ** marks differ\n    en: ${text}\n    es: ${es}`);
  }
}
const unused = Object.keys(ES).filter((text) => !used.has(text));

// ---- 4. messages from the Rust core ---------------------------------------------------

/** Spanish for a core message, the way ui/js/i18n.js does it. */
function spanishMessage(msg) {
  if (ES[msg]) return ES[msg];
  if (ES_MESSAGES[msg]) return ES_MESSAGES[msg];
  for (const [pattern, render] of ES_ERRORS) {
    const m = pattern.exec(msg);
    if (m) return render(m);
  }
  return null;
}

// Only a program can cause these (wrong arguments from an MCP client), so they
// are never shown by the app and stay in English.
const PROGRAM_ONLY = [
  /^Unknown (status|priority|method)/, /^Bad parameters/, /is not a date\. Write it as/, /^Which task\?/, /^Give both the pull request number/,
  /^A pull request needs a title/, /^The agenda window/, /^Headway is not available in/, /^Only web links can be opened/, /^That is not a valid GitHub user name/,
];

const untranslated = [];
let coreCount = 0;
const rustString = /"((?:[^"\\]|\\.)*)"/y;
for (const file of walk(coreDir, '.rs', (p) => /[\\/](target|tests)$/.test(p))) {
  const src = readFileSync(file, 'utf8').split(/\n#\[cfg\(test\)\]/)[0];
  const place = (index) => `${relative(root, file).replaceAll('\\', '/')}:${src.slice(0, index).split('\n').length}`;
  const seen = new Set();
  const take = (index, literalAt) => {
    rustString.lastIndex = literalAt;
    const m = rustString.exec(src);
    if (!m || seen.has(literalAt)) return;
    seen.add(literalAt);
    // Rust escapes and the "\<newline>" line continuation.
    const text = m[1].replace(/\\\n\s*/g, '').replace(/\\(["\\n])/g, (x, c) => (c === 'n' ? '\n' : c));
    // "{name}", "{}" and "{0}" become sample values; the pattern has to cope with them.
    const sample = text.replace(/\{[^{}]*\}/g, 'X1');
    if (!/[a-z]{3}/i.test(sample) || sample === 'X1') return;
    coreCount += 1;
    if (PROGRAM_ONLY.some((re) => re.test(sample))) return;
    if (!spanishMessage(sample)) untranslated.push([sample, place(index)]);
  };
  // Error::invalid("…"), Error::Other(format!("…")), #[error("…")], "…".to_string() inside an Error
  for (const m of src.matchAll(/Error::\w+\(\s*(?:format!\(\s*)?(?=")/g)) take(m.index, m.index + m[0].length);
  for (const m of src.matchAll(/#\[error\(\s*(?=")/g)) take(m.index, m.index + m[0].length);
  // A message built some other way is marked with a "// i18n" comment on the line before it.
  for (const m of src.matchAll(/\/\/ i18n[^\n]*\n[^\n"]*(?=")/g)) take(m.index, m.index + m[0].length);
}

// ---- report -----------------------------------------------------------------------------

if (process.argv.includes('--list')) {
  for (const [text, place] of [...used].sort((a, b) => a[1].localeCompare(b[1], 'en', { numeric: true }))) console.log(`${place}\t${JSON.stringify(text)}`);
  console.log();
}
for (const p of problems) console.log('PROBLEM  ' + p);
for (const [text, place] of missing) console.log(`MISSING  ${place}  ${JSON.stringify(text)}`);
for (const text of unused) console.log(`UNUSED   ${JSON.stringify(text)}`);
for (const [text, place] of untranslated) console.log(`CORE     ${place}  ${JSON.stringify(text)}`);

const bad = problems.length + missing.length + unused.length + untranslated.length;
console.log(`${used.size} texts in the UI, ${Object.keys(ES).length} in Spanish; ${coreCount} messages in the core.`);
console.log(bad ? `${bad} thing(s) to fix.` : 'Everything has a Spanish version.');
process.exit(bad ? 1 : 0);
