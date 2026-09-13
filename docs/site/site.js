const nav = document.getElementById('sidebar');
const main = document.querySelector('main');
const article = document.getElementById('article');
const toc = document.getElementById('toc');
const dialog = document.getElementById('search');
const input = document.getElementById('query');
const results = document.getElementById('results');
const menu = document.getElementById('menu');
const esc = text => String(text).replace(/[&<>"']/g, char => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[char]);

nav.innerHTML = groups.map(group => `<div class="nav-group"><h2>${esc(group.title)}</h2>${
  group.pages.map(slug => {
    const page = pages.find(page => page.slug === slug);
    return `<a href="#${slug}" data-page="${slug}">${esc(page.title)}</a>`;
  }).join('')
}</div>`).join('') + '<div class="sidebar-foot">TypeScript / Pi + Rust<br>Local state · source installation</div>';

let lastPage = '';
function closeNavigation() {
  nav.classList.remove('open');
  menu.setAttribute('aria-expanded', 'false');
}

function route() {
  const [slug, anchor] = (location.hash.slice(1) || 'index').split('~');
  const page = pages.find(page => page.slug === slug);
  if (!page) {
    article.innerHTML = '<h1>Page not found</h1><p><a href="#index">Return to the introduction</a> or search the documentation.</p>';
    toc.innerHTML = '';
    document.getElementById('paging').innerHTML = '';
    document.getElementById('source-note').innerHTML = '';
    document.getElementById('eyebrow').textContent = 'Documentation';
    document.title = 'Page not found · Ribosome docs';
    nav.querySelectorAll('a').forEach(link => {
      link.classList.remove('active');
      link.removeAttribute('aria-current');
    });
    lastPage = '';
    closeNavigation();
    return;
  }
  const changed = lastPage !== page.slug;
  lastPage = page.slug;
  document.title = `${page.title} · Ribosome docs`;
  document.getElementById('eyebrow').textContent = `${page.group} / ${page.title}`;
  article.innerHTML = page.html;
  nav.querySelectorAll('a').forEach(link => {
    const active = link.dataset.page === page.slug;
    link.classList.toggle('active', active);
    if (active) link.setAttribute('aria-current', 'page');
    else link.removeAttribute('aria-current');
  });
  toc.innerHTML = '<h2>On this page</h2>' + page.headings.map(heading =>
    `<a href="#${page.slug}~${heading.id}" class="${heading.level === 'h3' ? 'sub' : ''}">${esc(heading.text)}</a>`
  ).join('');
  const index = pages.indexOf(page);
  document.getElementById('paging').innerHTML = (index > 0
    ? `<a href="#${pages[index - 1].slug}"><span>Previous</span>← ${esc(pages[index - 1].title)}</a>` : '') +
    (index < pages.length - 1
      ? `<a class="next" href="#${pages[index + 1].slug}"><span>Next</span>${esc(pages[index + 1].title)} →</a>` : '');
  document.getElementById('source-note').innerHTML = `<a href="${page.source}">Edit this page on GitHub ↗</a>`;
  closeNavigation();
  if (anchor) requestAnimationFrame(() => document.getElementById(anchor)?.scrollIntoView({ behavior: 'instant' }));
  else if (changed) window.scrollTo({ top: 0, behavior: 'instant' });
  article.querySelectorAll('.copy').forEach(button => {
    button.onclick = async () => {
      const text = button.closest('.code-block').querySelector('code').textContent;
      try {
        await navigator.clipboard.writeText(text);
        button.textContent = 'Copied';
      } catch {
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        const copied = document.execCommand('copy');
        textarea.remove();
        button.textContent = copied ? 'Copied' : 'Select code';
      }
      setTimeout(() => { button.textContent = 'Copy'; }, 1300);
    };
  });
}

function search() {
  const terms = input.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const matches = pages.map(page => {
    const title = `${page.title} ${page.description}`.toLowerCase();
    const body = page.text.toLowerCase();
    const score = terms.reduce((score, term) => score + (title.includes(term) ? 6 : 0) + (body.includes(term) ? 1 : 0), 0);
    return { page, score, match: terms.every(term => title.includes(term) || body.includes(term)) };
  }).filter(result => result.match).sort((a, b) => b.score - a.score).slice(0, 12);
  results.innerHTML = matches.length ? matches.map(({ page }) =>
    `<a class="search-result" href="#${page.slug}"><strong>${esc(page.title)}</strong><small>${esc(page.group)} · ${esc(page.description)}</small></a>`
  ).join('') : '<div class="empty">No matching pages. Try “repair”, “memory”, or “provider”.</div>';
  results.querySelectorAll('a').forEach(link => { link.onclick = () => dialog.close(); });
}

function openSearch() {
  dialog.showModal();
  input.value = '';
  search();
  input.focus();
}

document.getElementById('search-toggle').onclick = openSearch;
document.getElementById('close-search').onclick = () => dialog.close();
input.addEventListener('input', search);
input.addEventListener('keydown', event => {
  if (event.key === 'Enter') {
    const first = results.querySelector('a');
    if (first) { location.hash = first.hash; dialog.close(); }
  } else if (event.key === 'ArrowDown') {
    event.preventDefault();
    results.querySelector('a')?.focus();
  }
});
dialog.addEventListener('click', event => { if (event.target === dialog) dialog.close(); });
document.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault();
    if (dialog.open) dialog.close();
    else openSearch();
  }
  if (event.key === 'Escape') closeNavigation();
});
menu.onclick = () => menu.setAttribute('aria-expanded', String(nav.classList.toggle('open')));
nav.addEventListener('click', event => { if (event.target.closest('a')) closeNavigation(); });
document.querySelector('.skip').addEventListener('click', event => {
  event.preventDefault();
  main.focus();
  main.scrollIntoView();
});
window.addEventListener('hashchange', route);
route();
