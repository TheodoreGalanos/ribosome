import { readFileSync, writeFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Script } from 'node:vm';
import { Marked, Parser, TextRenderer } from 'marked';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const docs = resolve(root, 'docs');
const content = resolve(docs, 'content');
const output = resolve(docs, 'index.html');
const repository = 'https://github.com/TheodoreGalanos/ribosome/blob/main/';
const { groups } = JSON.parse(readFileSync(resolve(docs, 'navigation.json'), 'utf8'));
const slugs = groups.flatMap(group => group.pages);
if (new Set(slugs).size !== slugs.length) throw new Error('Duplicate navigation page');
const files = readdirSync(content, { recursive: true })
  .filter(file => file.endsWith('.md')).map(file => file.slice(0, -3).split(sep).join('/'));
if (files.some(file => !slugs.includes(file)) || slugs.some(slug => !files.includes(slug))) {
  throw new Error('Navigation must list each content page exactly once');
}

const escape = text => String(text).replace(/[&<>"']/g, char => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[char]);
const slugify = text => text.toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');
const markdown = new Marked({ gfm: true, async: false });
const textRenderer = new TextRenderer();
const documents = new Map();

function readDocument(file) {
  if (documents.has(file)) return documents.get(file);
  const source = readFileSync(file, 'utf8');
  const frontmatter = /^---\n([\s\S]*?)\n---\n/.exec(source);
  const metadata = {};
  if (frontmatter) {
    for (const line of frontmatter[1].split('\n')) {
      const match = /^(title|description): (".*")$/.exec(line);
      if (!match) throw new Error(`Expected quoted title or description in ${file}: ${line}`);
      metadata[match[1]] = JSON.parse(match[2]);
    }
  }
  const body = source.slice(frontmatter?.[0].length ?? 0);
  const tokens = markdown.lexer(body);
  const headings = [];
  const used = new Set();
  markdown.walkTokens(tokens, token => {
    if (token.type !== 'heading') return;
    const text = new Parser().parseInline(token.tokens, textRenderer);
    const base = slugify(text);
    let id = base;
    for (let suffix = 1; used.has(id); suffix += 1) id = `${base}-${suffix}`;
    used.add(id);
    token.headingId = id;
    headings.push({ level: `h${token.depth}`, id, text });
  });
  const document = { ...metadata, tokens, headings, body };
  documents.set(file, document);
  return document;
}

const byFile = new Map(slugs.map(slug => [resolve(content, `${slug}.md`), slug]));
let localLinks = 0;
function resolveLink(href, from) {
  if (/^https?:\/\//.test(href) || href.startsWith('mailto:')) return href;
  if (/^[a-z][\w+.-]*:/i.test(href) || href.startsWith('//')) {
    throw new Error(`Unsupported documentation link in ${from}: ${href}`);
  }
  const [path, anchor] = href.split('#');
  const target = path ? resolve(dirname(from), decodeURIComponent(path)) : from;
  const repoPath = relative(root, target).split(sep).join('/');
  if (repoPath.startsWith('../') || (target !== output && !existsSync(target))) {
    throw new Error(`Missing local link in ${from}: ${href}`);
  }
  if (anchor && target.endsWith('.md') && !readDocument(target).headings.some(h => h.id === decodeURIComponent(anchor))) {
    throw new Error(`Missing heading in ${from}: ${href}`);
  }
  localLinks += 1;
  const slug = byFile.get(target);
  if (slug !== undefined) return `#${slug}${anchor ? `~${anchor}` : ''}`;
  const base = target !== output && statSync(target).isDirectory() ? repository.replace('/blob/', '/tree/') : repository;
  return `${base}${repoPath.split('/').map(encodeURIComponent).join('/')}${anchor ? `#${anchor}` : ''}`;
}

const pages = groups.flatMap(group => group.pages.map(slug => {
  const file = resolve(content, `${slug}.md`);
  const document = readDocument(file);
  if (!document.title || !document.description || document.headings.filter(h => h.level === 'h1').length !== 1) {
    throw new Error(`Each guide needs a title, description, and one H1: ${file}`);
  }
  const renderer = new Marked({ gfm: true, async: false, renderer: {
    heading({ tokens, depth, headingId }) {
      return `<h${depth} id="${escape(headingId)}">${this.parser.parseInline(tokens)}</h${depth}>\n`;
    },
    code({ text, lang }) {
      return `<div class="code-block"><div class="code-top"><span>${escape(lang || 'text')}</span><button class="copy" aria-label="Copy code">Copy</button></div><pre><code>${escape(text)}</code></pre></div>\n`;
    },
    link({ href, tokens, title }) {
      const url = resolveLink(href, file);
      return `<a href="${escape(url)}"${title ? ` title="${escape(title)}"` : ''}>${this.parser.parseInline(tokens)}</a>`;
    },
    image({ href, text }) {
      return `<img src="${escape(resolveLink(href, file))}" alt="${escape(text)}">`;
    },
    html({ text }) { return escape(text); },
  } });
  return {
    slug, title: document.title, description: document.description, group: group.title,
    html: renderer.parser(document.tokens, renderer.defaults), text: document.body,
    headings: document.headings.filter(h => h.level === 'h2' || h.level === 'h3'),
    source: repository + relative(root, file).split(sep).join('/'),
  };
}));

// The Markdown entry points must keep working on GitHub as well.
for (const file of [resolve(root, 'README.md'), resolve(docs, 'README.md')]) {
  markdown.walkTokens(readDocument(file).tokens, token => {
    if (token.type === 'link' || token.type === 'image') resolveLink(token.href, file);
  });
}

const template = readFileSync(resolve(docs, 'site/template.html'), 'utf8');
const script = readFileSync(resolve(docs, 'site/site.js'), 'utf8');
const json = value => JSON.stringify(value).replace(/</g, '\\u003c');
const html = template.replace('__PAGES__', () => json(pages))
  .replace('__GROUPS__', () => json(groups)).replace('__SCRIPT__', () => script);
new Script(html.match(/<script>([\s\S]*?)<\/script>/)[1], { filename: 'docs/index.html' });
if (!process.argv.includes('--check')) writeFileSync(output, html);
console.log(`${pages.length} documentation pages rendered; ${localLinks} local links and their Markdown anchors checked.`);
