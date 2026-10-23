'use strict';

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog;

const $ = (id) => document.getElementById(id);

const state = {
  dir: '',          // a folder that was chosen (its files are listed), or
  paths: [],        // files that were chosen one by one
  files: [],        // what was read from them: { path, name, container, tracks, error }
  selected: new Set(),
  expanded: new Set(),
  running: false,
  languages: [],
  home: '',
  again: null,      // the request that applies what a preview showed
};

// What each action needs from the form. `none` offers "no default" and `keep` offers
// "leave the language as it is" in the language list.
const ACTIONS = {
  'default-lang': { show: ['kind', 'lang', 'prefer', 'move'] },
  'track-default': { show: ['kind', 'lang', 'track', 'move'], keep: true },
  'set-lang': { show: ['kind', 'lang', 'track'] },
  'rename': { show: ['kind', 'name', 'track'] },
  'forced': { show: ['kind', 'forcedflag', 'track'] },
  'swap': { show: [] },
  'delete-lang': { show: ['kind', 'lang'] },
  'delete-track': { show: ['kind', 'track'] },
  'preserve': { show: ['kind', 'lang', 'prefer'] },
};

function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (key === 'class') node.className = value;
    else node.setAttribute(key, value);
  }
  for (const child of children) {
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}

const words = (text) => text.split(',').map((s) => s.trim()).filter(Boolean);
const baseName = (path) => path.split(/[\\/]/).pop();

// Form --------------------------------------------------------------------------

function fillLanguages() {
  const spec = ACTIONS[$('action').value];
  const select = $('lang');
  const before = select.value;
  select.replaceChildren();
  if (spec.keep) select.append(el('option', { value: '' }, '(leave as it is)'));
  if ($('action').value === 'default-lang' && $('kind').value === 'sub') {
    select.append(el('option', { value: 'none' }, '(none: no default subtitles)'));
  }
  for (const { code, name } of state.languages) select.append(el('option', { value: code }, name));
  select.value = [...select.options].some((o) => o.value === before)
    ? before : (spec.keep ? '' : 'eng');
}

function updateForm() {
  const action = $('action').value;
  const spec = ACTIONS[action];
  const sub = $('kind').value === 'sub' && action !== 'swap';
  const visible = {
    kind: spec.show.includes('kind'),
    lang: spec.show.includes('lang'),
    prefer: spec.show.includes('prefer'),
    track: spec.show.includes('track'),
    name: spec.show.includes('name'),
    forcedflag: spec.show.includes('forcedflag'),
    move: spec.show.includes('move'),
    forcedtrack: action === 'default-lang' && sub,
  };
  for (const node of document.querySelectorAll('[data-for]')) {
    node.hidden = !visible[node.dataset.for];
  }
  $('lang-label').textContent = action === 'delete-lang' ? 'Remove this language'
    : action === 'preserve' ? 'Keep this language' : 'Language';
  fillLanguages();
  $('move').disabled = $('lang').value === 'none';
}

function request(dry) {
  const action = $('action').value;
  const kind = action === 'swap' ? 'audio' : $('kind').value;
  const r = {
    paths: state.files.filter((f) => !f.error && state.selected.has(f.path)).map((f) => f.path),
    kind,
    dryrun: dry,
    backup: $('backup').checked,
    in_place: $('in-place').checked,
    force: $('force').checked,
    jobs: Math.max(1, parseInt($('jobs').value, 10) || 1),
    log: $('use-log').checked ? $('log').value.trim() : '',
    tasks: [],
  };
  const lang = $('lang').value;
  const prefer = words($('prefer').value);
  const track = $('track').value.trim();
  switch (action) {
    case 'default-lang':
      r.tasks = [{ name: 'set', what: 'lang' }];
      if (prefer.length) r.prefer = prefer; else r.default = lang;
      r.forced = $('forced').checked && kind === 'sub';
      r.move_first = $('move').checked && lang !== 'none';
      break;
    case 'track-default':
      r.tasks = [{ name: 'set', what: 'default' }];
      r.track = track; r.lang = lang; r.move_first = $('move').checked;
      break;
    case 'set-lang':
      r.tasks = [{ name: 'set', what: 'lang' }]; r.track = track; r.lang = lang;
      break;
    case 'rename':
      r.tasks = [{ name: 'set', what: 'name' }]; r.track = track; r.name = $('name').value;
      break;
    case 'forced':
      r.tasks = [{ name: 'set', what: $('forcedflag').value }]; r.track = track;
      break;
    case 'swap':
      r.tasks = [{ name: 'swap', what: 'lang' }];
      break;
    case 'delete-lang':
      r.tasks = [{ name: 'delete', what: 'lang' }]; r.lang = lang;
      break;
    case 'delete-track':
      r.tasks = [{ name: 'delete', what: 'track' }]; r.track = track;
      break;
    case 'preserve':
      r.tasks = [{ name: 'preserve', what: 'lang' }];
      if (prefer.length) r.prefer = prefer; else r.lang = lang;
      break;
  }
  return r;
}

// Files -------------------------------------------------------------------------

function trackSummary(tracks, kind) {
  const list = tracks.filter((t) => t.type === kind);
  return list.length
    ? list.map((t) => t.language + (t.default ? '*' : '') + (t.forced ? ' (forced)' : '')).join(', ')
    : '-';
}

function renderFiles() {
  const body = $('files').querySelector('tbody');
  body.replaceChildren();
  $('files-wrap').hidden = state.files.length === 0;
  for (const file of state.files) {
    const box = el('input', { type: 'checkbox', 'aria-label': `Select ${file.name}` });
    box.checked = state.selected.has(file.path);
    box.disabled = Boolean(file.error);
    box.addEventListener('click', (e) => e.stopPropagation());
    box.addEventListener('change', () => {
      if (box.checked) state.selected.add(file.path); else state.selected.delete(file.path);
      updateButtons();
    });
    const row = el('tr', { class: 'file' },
      el('td', { class: 'narrow' }, box),
      el('td', { class: 'name' }, file.name),
      el('td', {}, file.error ? '' : file.container),
      file.error
        ? el('td', { colspan: 2, class: 'muted' }, file.error)
        : el('td', {}, trackSummary(file.tracks, 'audio')),
      ...(file.error ? [] : [el('td', {}, trackSummary(file.tracks, 'subtitles'))]));
    row.addEventListener('click', () => {
      if (state.expanded.has(file.path)) state.expanded.delete(file.path);
      else state.expanded.add(file.path);
      renderFiles();
    });
    body.append(row);
    if (state.expanded.has(file.path) && !file.error) body.append(trackDetail(file));
  }
  $('select-all').checked = state.files.length > 0
    && state.files.every((f) => f.error || state.selected.has(f.path));
  updateButtons();
}

function trackDetail(file) {
  const table = el('table', { class: 'tracks' },
    el('thead', {}, el('tr', {}, ...['Track', 'Codec', 'Language', 'Default', 'Forced', 'Name']
      .map((h) => el('th', {}, h)))),
    el('tbody', {}, ...file.tracks.map((t) => el('tr', {},
      el('td', {}, `${t.type}:${t.number}`), el('td', {}, t.codec), el('td', {}, t.language),
      el('td', {}, t.default ? 'yes' : '-'), el('td', {}, t.forced ? 'yes' : '-'),
      el('td', {}, t.name)))));
  return el('tr', { class: 'detail' }, el('td', {}), el('td', { colspan: 4 }, table));
}

async function scan() {
  const r = { kind: 'audio' };
  if (state.dir) {
    Object.assign(r, { dir: state.dir, recursive: $('recursive').checked,
      include: words($('include').value), exclude: words($('exclude').value) });
  } else if (state.paths.length) {
    r.paths = state.paths;
  } else {
    return;
  }
  setBusy(true, 'Reading the files…');
  try {
    const summary = await invoke('scan', { request: r });
    const slashes = (p) => p.replace(/\\/g, '/');  // Windows paths use backslashes
    const prefix = state.dir ? slashes(state.dir).replace(/\/+$/, '') + '/' : '';
    state.files = summary.results.filter((x) => x.action === 'info').map((x) => ({
      path: x.file,
      name: prefix && slashes(x.file).startsWith(prefix)
        ? slashes(x.file).slice(prefix.length) : baseName(x.file),
      container: x.data ? x.data.container : '',
      tracks: x.data ? x.data.tracks : [],
      error: x.status === 'error' ? x.message : '',
    }));
    state.selected = new Set(state.files.filter((f) => !f.error).map((f) => f.path));
    state.expanded.clear();
    const usable = state.files.filter((f) => !f.error).length;
    $('source').textContent = (state.dir ? `Folder: ${state.dir}` : 'Chosen files')
      + `: ${usable} media file${usable === 1 ? '' : 's'} found.`;
    renderFiles();
  } catch (error) {
    $('source').textContent = `Could not read the files: ${error}`;
  } finally {
    setBusy(false);
  }
}

async function addFolder() {
  const chosen = await open({ directory: true, multiple: false });
  if (!chosen) return;
  state.dir = chosen; state.paths = [];
  await scan();
}

async function addFiles() {
  const chosen = await open({ multiple: true, filters: [
    { name: 'Media files', extensions: ['mkv', 'mp4', 'm4v', 'mov', 'avi', 'webm'] }] });
  if (!chosen || !chosen.length) return;
  state.paths = Array.isArray(chosen) ? chosen : [chosen]; state.dir = '';
  await scan();
}

// Running -----------------------------------------------------------------------

function setBusy(busy, message) {
  state.running = busy;
  if (busy && message) $('source').textContent = message;
  $('cancel').hidden = !busy;
  updateButtons();
}

function updateButtons() {
  const chosen = state.files.some((f) => !f.error && state.selected.has(f.path));
  for (const id of ['preview', 'apply']) $(id).disabled = state.running || !chosen;
  for (const id of ['undo', 'add-folder', 'add-files', 'rescan']) $(id).disabled = state.running;
  $('apply-preview').disabled = state.running;
}

function confirmApply(text) {
  $('confirm-text').textContent = text;
  const dialog = $('confirm');
  return new Promise((resolve) => {
    dialog.addEventListener('close', () => resolve(dialog.returnValue === 'apply'), { once: true });
    dialog.showModal();
  });
}

function statusLabel(r) {
  if (r.status === 'changed' && r.dry_run) return 'would change';
  return r.status === 'ok' ? 'done' : r.status;
}

function addResult(r) {
  if (r.action === 'check') return;
  const lines = r.changes.length ? r.changes : (r.message ? [r.message] : []);
  const item = el('li', { class: `s-${r.status}` },
    el('div', {}, el('span', { class: 'status' }, statusLabel(r)),
      el('span', { class: 'file' }, baseName(r.file))),
    ...(lines.length ? [el('ul', { class: 'lines' }, ...lines.map((l) => el('li', {}, l)))] : []));
  item.firstChild.title = r.file;
  $('results').append(item);
  const bar = $('progress');
  bar.value = Math.min(bar.max, bar.value + 1);
}

function showSummary(results, dry) {
  const count = (s) => results.filter((r) => r.status === s).length;
  const parts = [];
  const changed = count('changed');
  parts.push(`${changed} ${dry ? 'would change' : 'changed'}`);
  for (const s of ['unchanged', 'skipped', 'error']) {
    if (count(s)) parts.push(`${count(s)} ${s === 'error' ? (count(s) === 1 ? 'error' : 'errors') : s}`);
  }
  $('summary').textContent = parts.join(', ');
  return changed;
}

async function go(dry, req = null) {
  const r = req || request(dry);
  if (!r.paths.length && !r.tasks.some((t) => t.name === 'undo')) return;
  $('results-card').hidden = false;
  $('results').replaceChildren();
  $('warnings').hidden = true; $('warnings').textContent = '';
  $('apply-preview').hidden = true;
  $('summary').textContent = dry ? 'Previewing…' : 'Working…';
  const bar = $('progress');
  bar.max = Math.max(1, r.paths.length); bar.value = 0; bar.hidden = r.paths.length === 0;
  $('results-card').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  setBusy(true);
  try {
    const summary = await invoke('run', { request: r });
    const changed = showSummary(summary.results, dry);
    if (summary.code === 130) $('summary').textContent += ' (stopped)';
    if (dry && changed > 0) {
      state.again = { ...r, dryrun: false };
      $('apply-preview').hidden = false;
    }
    if (!dry) await scan();
  } catch (error) {
    $('summary').textContent = String(error);
  } finally {
    bar.hidden = true;
    setBusy(false);
  }
}

async function apply(req) {
  const r = req || request(false);
  const what = r.tasks.some((t) => t.name === 'undo')
    ? 'Undo the changes recorded in the log?'
    : `Change ${r.paths.length} file${r.paths.length === 1 ? '' : 's'}?`;
  if (!(await confirmApply(what))) return;
  await go(false, r);
}

async function undo() {
  const chosen = await open({ multiple: false, filters: [
    { name: 'fml logs', extensions: ['jsonl', 'log', 'json', 'txt'] }, { name: 'All files', extensions: ['*'] }] });
  if (!chosen) return;
  await go(true, { tasks: [{ name: 'undo', what: chosen }], dryrun: true, paths: [] });
}

// Start up ----------------------------------------------------------------------

async function start() {
  const info = await invoke('app_info');
  $('version').textContent = `v${info.version}`;
  const missing = info.tools.filter((t) => !t.path).map((t) => t.tool);
  const chip = $('tools');
  chip.textContent = missing.length ? `Missing: ${missing.join(', ')}` : 'mkvtoolnix, ffmpeg, mediainfo found';
  chip.classList.add(missing.length ? 'bad' : 'good');
  chip.title = info.tools.map((t) => `${t.tool}: ${t.path || 'not found'}`).join('\n');
  state.home = info.home;
  $('log').value = `${info.home || '.'}/fml-log.jsonl`;
  state.languages = await invoke('languages');
  updateForm();
  updateButtons();

  await listen('fml-result', (event) => addResult(event.payload));
  await listen('fml-warn', (event) => {
    $('warnings').hidden = false;
    $('warnings').textContent += `${event.payload}\n`;
  });
}

$('action').addEventListener('change', updateForm);
$('kind').addEventListener('change', updateForm);
$('lang').addEventListener('change', () => { $('move').disabled = $('lang').value === 'none'; });
$('add-folder').addEventListener('click', addFolder);
$('add-files').addEventListener('click', addFiles);
$('rescan').addEventListener('click', scan);
$('select-all').addEventListener('change', (e) => {
  state.selected = new Set(e.target.checked ? state.files.filter((f) => !f.error).map((f) => f.path) : []);
  renderFiles();
});
$('preview').addEventListener('click', () => go(true));
$('apply').addEventListener('click', () => apply());
$('apply-preview').addEventListener('click', () => apply(state.again));
$('undo').addEventListener('click', undo);
$('cancel').addEventListener('click', () => invoke('cancel'));

start().catch((error) => { $('source').textContent = `Could not start: ${error}`; });
