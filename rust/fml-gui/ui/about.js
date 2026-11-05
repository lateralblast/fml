(() => {
  'use strict';

  const { invoke } = window.__TAURI__.core;
  const $ = (id) => document.getElementById(id);

  // One paragraph of ABOUT.md (the first block is the title). Its links (<a href title>) open in the browser.
  // Built from DOM nodes only, so the text is never read as HTML.
  function paragraph(block, first) {
    const node = document.createElement(first ? 'h2' : 'p');
    const pattern = /<a\s+href="([^"]+)"(?:\s+title="([^"]*)")?>(.*?)<\/a>/g;
    let last = 0;
    let match;
    while ((match = pattern.exec(block)) !== null) {
      node.append(document.createTextNode(block.slice(last, match.index)));
      const [, href, title, text] = match;
      const link = document.createElement('a');
      link.href = href;
      link.title = title || href;
      link.textContent = text;
      link.addEventListener('click', (e) => {
        e.preventDefault();
        invoke('open_link', { url: href }).catch((error) => { $('about-status').textContent = String(error); });
      });
      node.append(link);
      last = pattern.lastIndex;
    }
    node.append(document.createTextNode(block.slice(last)));
    return node;
  }

  async function load() {
    const blocks = (await invoke('about_text')).split(/\n\s*\n/).map((b) => b.trim()).filter(Boolean);
    $('about-body').replaceChildren(...blocks.map((block, index) => paragraph(block, index === 0)));
    const info = await invoke('app_info');
    $('about-version').textContent = `Version ${info.version}`;
  }

  load().catch((error) => { $('about-status').textContent = `Could not load: ${error}`; });
})();
