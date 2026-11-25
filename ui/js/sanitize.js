// GitHub renders issue text to HTML for us. Before that HTML is put on the
// page it passes through this allowlist: only plain formatting survives, links
// can only be web links, and nothing can run script.

const TAGS = new Set([
  'p', 'br', 'hr', 'strong', 'b', 'em', 'i', 'del', 's', 'code', 'pre', 'blockquote', 'ul', 'ol', 'li', 'a', 'img',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'table', 'thead', 'tbody', 'tr', 'th', 'td', 'input', 'span', 'div', 'kbd',
  'sup', 'sub', 'details', 'summary',
]);
const DROP_WITH_CONTENT = new Set(['script', 'style', 'iframe', 'object', 'embed', 'template', 'svg', 'math', 'form', 'button', 'textarea', 'select', 'video', 'audio', 'link', 'meta', 'base']);
const KEEP_CLASSES = new Set(['contains-task-list', 'task-list-item', 'task-list-item-checkbox']);

const webUrl = (u) => /^https?:\/\//i.test(u) || /^mailto:/i.test(u);

function clean(node, out) {
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === 3) {
      out.appendChild(document.createTextNode(child.nodeValue));
      continue;
    }
    if (child.nodeType !== 1) continue;
    const tag = child.tagName.toLowerCase();
    if (DROP_WITH_CONTENT.has(tag)) continue;
    if (!TAGS.has(tag)) {
      clean(child, out); // unknown wrapper: keep its text
      continue;
    }
    const el = document.createElement(tag);
    if (tag === 'a') {
      const href = child.getAttribute('href') || '';
      if (webUrl(href)) {
        el.setAttribute('href', href);
        el.setAttribute('rel', 'noopener noreferrer');
        el.setAttribute('data-ext', '');
      }
    } else if (tag === 'img') {
      const src = child.getAttribute('src') || '';
      if (!/^https:\/\//i.test(src)) continue;
      el.setAttribute('src', src);
      el.setAttribute('alt', child.getAttribute('alt') || '');
      el.setAttribute('loading', 'lazy');
    } else if (tag === 'input') {
      if ((child.getAttribute('type') || '').toLowerCase() !== 'checkbox') continue;
      el.setAttribute('type', 'checkbox');
      el.setAttribute('disabled', '');
      if (child.hasAttribute('checked')) el.setAttribute('checked', '');
    } else if (tag === 'td' || tag === 'th') {
      const align = (child.getAttribute('align') || '').toLowerCase();
      if (align === 'left' || align === 'right' || align === 'center') el.setAttribute('align', align);
    }
    const classes = (child.getAttribute('class') || '').split(/\s+/).filter((c) => KEEP_CLASSES.has(c));
    if (classes.length) el.setAttribute('class', classes.join(' '));
    clean(child, el);
    out.appendChild(el);
  }
}

const cache = new Map();

/** Returns safe HTML for GitHub-rendered Markdown. */
export function sanitize(html) {
  if (!html) return '';
  const hit = cache.get(html);
  if (hit !== undefined) return hit;
  const doc = new DOMParser().parseFromString(String(html), 'text/html');
  const out = document.createElement('div');
  clean(doc.body, out);
  const safe = out.innerHTML;
  if (cache.size > 300) cache.clear();
  cache.set(html, safe);
  return safe;
}
