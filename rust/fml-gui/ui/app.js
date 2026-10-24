'use strict';

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open, save } = window.__TAURI__.dialog;

const $ = (id) => document.getElementById(id);

const state = {
  dir: '',          // a folder that was chosen (its files are listed), or
  paths: [],        // files that were chosen one by one
  files: [],        // what was read from them: { path, name, container, tracks, error }
  selected: new Set(),
  removed: new Set(), // files taken off the list, kept off by a rescan of the same folder or files
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

// The settings every run takes from the form: the files, and the safety and speed options.
function commonRequest(dry) {
  return {
    paths: state.files.filter((f) => !f.error && state.selected.has(f.path)).map((f) => f.path),
    dryrun: dry,
    backup: $('backup').checked,
    in_place: $('in-place').checked,
    force: $('force').checked,
    jobs: Math.max(1, parseInt($('jobs').value, 10) || 1),
    log: $('use-log').checked ? $('log').value.trim() : '',
    tasks: [],
  };
}

function request(dry) {
  const action = $('action').value;
  const kind = action === 'swap' ? 'audio' : $('kind').value;
  const r = { ...commonRequest(dry), kind };
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
      ...(file.error ? [] : [el('td', {}, trackSummary(file.tracks, 'subtitles'))]),
      el('td', { class: 'edit-cell' }, file.error ? '' : infoButton(file)));
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

function trackTable(table, tracks) {
  table.replaceChildren(
    el('thead', {}, el('tr', {}, ...['Track', 'ID', 'Codec', 'Language', 'Default', 'Forced', 'Name']
      .map((h) => el('th', {}, h)))),
    el('tbody', {}, ...tracks.map((t) => el('tr', {},
      el('td', {}, `${t.type}:${t.number}`), el('td', {}, t.id), el('td', {}, t.codec),
      el('td', {}, t.language), el('td', {}, t.default ? 'yes' : '-'),
      el('td', {}, t.forced ? 'yes' : '-'), el('td', {}, t.name)))));
  return table;
}

function trackDetail(file) {
  const table = trackTable(el('table', { class: 'tracks' }), file.tracks);
  return el('tr', { class: 'detail' }, el('td', {}), el('td', { colspan: 5 }, table));
}

// The Edit button of a row: the file's facts, its tracks and mediainfo's full report.
function infoButton(file) {
  const button = el('button', { type: 'button', class: 'edit', 'aria-label': `Edit and see details of ${file.name}` }, 'Edit');
  button.addEventListener('click', (e) => { e.stopPropagation(); showInfo(file); });
  return button;
}

function sizeText(bytes) {
  const units = ['bytes', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes, unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1; }
  return unit === 0 ? `${bytes} bytes` : `${value.toFixed(1)} ${units[unit]} (${bytes.toLocaleString()} bytes)`;
}

// The tracks in the Info dialog, in the order being set up: audio and subtitle tracks can be
// moved up and down and the new order applied (one rewrite of the file per kind).
const info = { file: null, order: {}, picked: {}, busy: false, subFile: '', defaultFirst: {}, edit: {}, keep: {} };

const KIND_TITLES = { video: 'Video', audio: 'Audio tracks', subtitles: 'Subtitle tracks' };

function resetOrder(file) {
  info.file = file;
  info.order = {}; info.picked = {}; info.defaultFirst = { audio: false, subtitles: false }; info.edit = {}; info.keep = { audio: '', subtitles: '' };
  for (const kind of ['audio', 'subtitles']) info.order[kind] = file.tracks.filter((t) => t.type === kind);
}

const orderChanged = (kind) => info.order[kind].some((t, i) => t.number !== i + 1);

function moveTrack(kind, by) {
  const list = info.order[kind];
  const at = list.findIndex((t) => t.number === info.picked[kind]);
  const to = at + by;
  if (at < 0 || to < 0 || to >= list.length) return;
  [list[at], list[to]] = [list[to], list[at]];
  renderOrder();
}

function defaultFirstBox(kind) {
  const box = el('input', { type: 'checkbox' });
  box.checked = info.defaultFirst[kind]; box.disabled = info.busy;
  box.addEventListener('change', () => { info.defaultFirst[kind] = box.checked; renderOrder(); });
  return el('label', { class: 'check', title: 'The track that ends up first also gets the default flag, and the others lose it' },
    box, ' Make the first track the default');
}

// Keep only one language: the tracks in the others are marked in the table as they would be
// removed, with what happens to the default flag, before anything is changed.
function keepSummary(kind, tracks, keep) {
  const gone = tracks.filter((t) => t.language !== keep);
  const left = tracks.filter((t) => t.language === keep);
  if (!gone.length) return `Every ${kindWord(kind)} track is already in ${keep}: nothing to remove.`;
  let text = `Would remove ${gone.length} ${kindWord(kind)} track${gone.length === 1 ? '' : 's'} (${gone.map((t) => t.number).join(', ')}), keeping ${left.length}.`;
  const promoted = !left.some((t) => t.default) && (kind === 'audio' || info.file.container === 'MP4');
  if (gone.some((t) => t.default) && promoted) text += ` Track ${left[0].number} becomes the default.`;
  return text;
}

function keepRow(kind, tracks) {
  const languages = [...new Set(info.file.tracks.filter((t) => t.type === kind).map((t) => t.language))];
  const select = el('select', { 'aria-label': `Language to keep among the ${kindWord(kind)} tracks` },
    el('option', { value: '' }, '(keep all)'),
    ...languages.map((code) => {
      const known = state.languages.find((l) => l.code === code);
      return el('option', { value: code }, known ? `${known.name} (${code})` : code);
    }));
  select.value = info.keep[kind];
  select.disabled = info.busy || languages.length < 2;
  select.addEventListener('change', () => { info.keep[kind] = select.value; renderOrder(); });
  const keep = info.keep[kind];
  const selected = commonRequest(true).paths.length;
  const remove = el('button', { type: 'button', class: 'danger' }, 'Remove the others…');
  remove.disabled = !keep || info.busy || orderChanged(kind) || !tracks.some((t) => t.language !== keep);
  remove.addEventListener('click', () => keepOnly(kind, keep));
  const all = el('button', { type: 'button' }, 'Preview for all ticked files…');
  all.disabled = !keep || info.busy || selected === 0;
  all.addEventListener('click', () => previewForAll(kind, 'keep'));
  return el('div', { class: 'row order-tools' }, el('label', {}, 'Keep only ', select), remove, all,
    el('span', { class: keep ? '' : 'muted' }, keep ? keepSummary(kind, tracks, keep)
      : (languages.length < 2 ? `All the ${kindWord(kind)} tracks are in one language.` : 'Choose a language to see what would go.')));
}

function keepOnly(kind, keep) {
  const tracks = info.order[kind];
  return changeFile({ kind: kind === 'audio' ? 'audio' : 'sub', tasks: [{ name: 'preserve', what: 'lang' }], lang: keep },
    `Remove the ${kindWord(kind)} tracks that are not in ${keep} from ${info.file.name}? ${keepSummary(kind, tracks, keep)}`);
}

// Edit the language, name and forced flag of the picked track; the changes are made in one go.
function trackEditor(kind, track, canForce) {
  const saved = info.edit[kind];
  const edit = saved && saved.number === track.number ? saved
    : (info.edit[kind] = { number: track.number, lang: track.language, name: track.name, forced: track.forced });
  const lang = el('select', { 'aria-label': 'Language of the track' });
  const codes = state.languages.map((l) => l.code);
  if (!codes.includes(track.language)) lang.append(el('option', { value: track.language }, track.language));
  for (const l of state.languages) lang.append(el('option', { value: l.code }, l.name));
  lang.value = edit.lang;
  const name = el('input', { type: 'text', size: '24', placeholder: 'empty removes the name', 'aria-label': 'Name of the track' });
  name.value = edit.name;
  const forced = el('input', { type: 'checkbox' });
  forced.checked = edit.forced;
  const go = el('button', { type: 'button', class: 'primary' }, 'Save changes…');
  const dirty = () => edit.lang !== track.language || edit.name.trim() !== track.name || (canForce && edit.forced !== track.forced);
  const refresh = () => { go.disabled = info.busy || orderChanged(kind) || !dirty(); };
  lang.addEventListener('change', () => { edit.lang = lang.value; refresh(); });
  name.addEventListener('input', () => { edit.name = name.value; refresh(); });
  forced.addEventListener('change', () => { edit.forced = forced.checked; refresh(); });
  go.addEventListener('click', () => saveTrack(kind, track, edit, canForce));
  const extension = kind === 'subtitles' ? subtitleExtension(info.file.container, track.codec) : '';
  const extract = el('button', { type: 'button',
    title: extension ? `Write this subtitle track to a .${extension} file` : "This kind of subtitle can't be written to a single file" },
  'Extract…');
  extract.disabled = info.busy || !extension;
  extract.addEventListener('click', () => extractTrack(track, extension));
  refresh();
  return el('div', { class: 'row track-editor' },
    el('strong', {}, `${kindWord(kind)} track ${track.number}`),
    el('label', {}, 'Language ', lang), el('label', {}, 'Name ', name),
    ...(canForce ? [el('label', { class: 'check' }, forced, ' Forced')] : []), go,
    ...(kind === 'subtitles' ? [extract] : []),
    ...(orderChanged(kind) ? [el('span', { class: 'muted' }, 'Apply or reset the new order first.')] : []));
}

// The extension a subtitle track is written with (as the engine does), or '' if it can't be.
function subtitleExtension(container, codec) {
  const c = codec.toLowerCase();
  if (container === 'MP4') return c.includes('timed text') || c.includes('tx3g') || c.includes('mov_text') ? 'srt' : '';
  if (c.includes('subrip') || c.includes('srt')) return 'srt';
  if (c.includes('substation')) return 'ass';
  if (c.includes('webvtt')) return 'vtt';
  if (c.includes('pgs')) return 'sup';
  return '';
}

// Write a subtitle track to a file of its own. The save dialog is the question, so a file that
// is chosen there is replaced.
async function extractTrack(track, extension, chosenPath = null) {
  const file = info.file;
  const cut = Math.max(file.path.lastIndexOf('/'), file.path.lastIndexOf('\\'));
  const stem = baseName(file.path).replace(/\.[^.]*$/, '');
  const suggested = `${file.path.slice(0, cut + 1)}${stem}.${track.language}${track.forced ? '.forced' : ''}.${extension}`;
  const chosen = chosenPath || await save({ defaultPath: suggested, filters: [{ name: 'Subtitle file', extensions: [extension] }] });
  if (!chosen) return;
  info.busy = true; setBusy(true); renderOrder();
  $('info-status').textContent = 'Working…';
  try {
    const summary = await invoke('run', { request: {
      paths: [file.path], kind: 'sub', dryrun: false, force: true, output: chosen,
      tasks: [{ name: 'extract', what: 'subtitle' }], track: `subtitles:${track.number}` } });
    const r = summary.results[0];
    const lines = r && r.changes.length ? r.changes.join('; ') : (r && r.message) || '';
    $('info-status').textContent = r ? `${statusLabel(r)}${lines ? `: ${lines}` : ''}` : 'Nothing was done.';
  } catch (error) {
    $('info-status').textContent = String(error);
  } finally {
    info.busy = false; setBusy(false); renderOrder();
  }
}

function saveTrack(kind, track, edit, canForce) {
  const request = { kind: kind === 'audio' ? 'audio' : 'sub', tasks: [{ name: 'set', what: 'track' }],
    track: `${kind}:${track.number}` };
  const parts = [];
  if (edit.lang !== track.language) {
    request.lang = edit.lang;
    const l = state.languages.find((x) => x.code === edit.lang);
    parts.push(`language ${l ? l.name : edit.lang}`);
  }
  if (edit.name.trim() !== track.name) {
    request.name = edit.name.trim();
    parts.push(request.name ? `name "${request.name}"` : 'no name');
  }
  if (canForce && edit.forced !== track.forced) {
    request.forced_flag = edit.forced;
    parts.push(edit.forced ? 'forced' : 'not forced');
  }
  if (!parts.length) return Promise.resolve();
  return changeFile(request, `Change ${kindWord(kind)} track ${track.number} of ${info.file.name}: ${parts.join(', ')}?`);
}

function renderOrder() {
  const box = $('info-tracks');
  box.replaceChildren();
  const file = info.file;
  const editable = file.container !== 'AVI' && file.container !== 'avi';
  for (const kind of ['video', 'audio', 'subtitles']) {
    const tracks = kind === 'video' ? file.tracks.filter((t) => t.type === kind) : info.order[kind];
    if (!tracks.length) continue;
    const movable = kind !== 'video';
    const heading = el('h4', {}, KIND_TITLES[kind]);
    const keep = movable ? info.keep[kind] : '';
    const head = ['#', ...(movable ? ['Was'] : []), 'ID', 'Codec', 'Language', 'Default', 'Forced', 'Name', ...(keep ? ['After'] : [])];
    const rows = tracks.map((t, i) => {
      const gone = Boolean(keep) && t.language !== keep;
      const row = el('tr', movable && editable ? { tabindex: '0', 'aria-selected': String(info.picked[kind] === t.number) } : {},
        el('td', {}, i + 1), ...(movable ? [el('td', { class: t.number === i + 1 ? 'muted' : '' }, t.number)] : []),
        el('td', {}, t.id), el('td', {}, t.codec), el('td', {}, t.language),
        el('td', {}, t.default ? 'yes' : '-'), el('td', {}, t.forced ? 'yes' : '-'), el('td', {}, t.name),
        ...(keep ? [el('td', { class: gone ? 'gone-label' : 'muted' }, gone ? 'removed' : 'kept')] : []));
      if (gone) row.classList.add('removing');
      if (movable && editable) {
        const pick = () => { info.picked[kind] = t.number; renderOrder(); };
        row.addEventListener('click', pick);
        row.addEventListener('keydown', (e) => {
          if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); pick(); }
        });
        if (info.picked[kind] === t.number) row.classList.add('picked');
      }
      return row;
    });
    const table = el('table', { class: 'tracks' },
      el('thead', {}, el('tr', {}, ...head.map((h) => el('th', {}, h)))), el('tbody', {}, ...rows));
    box.append(heading);
    if (movable && editable) {
      const at = tracks.findIndex((t) => t.number === info.picked[kind]);
      const button = (label, onClick, disabled, cls = '') => {
        const b = el('button', { type: 'button', class: cls }, label);
        b.disabled = disabled || info.busy; b.addEventListener('click', onClick); return b;
      };
      box.append(el('div', { class: 'row order-tools' },
        button('Move up', () => moveTrack(kind, -1), at <= 0),
        button('Move down', () => moveTrack(kind, 1), at < 0 || at === tracks.length - 1),
        button('Delete…', () => deleteTrack(kind), at < 0 || orderChanged(kind), 'danger'),
        button('Reset', () => { info.order[kind] = file.tracks.filter((t) => t.type === kind); renderOrder(); }, !orderChanged(kind), 'quiet'),
        button('Apply this order…', () => applyOrder(kind), !orderChanged(kind) && !info.defaultFirst[kind], 'primary'),
        defaultFirstBox(kind),
        el('span', { class: 'muted' }, orderChanged(kind) ? 'Apply or reset the new order before deleting.' : 'Click a track, then move or delete it.')));
    }
    if (movable && editable) {
      const selected = commonRequest(true).paths.length;
      const wide = (label, onClick, disabled, cls = '') => {
        const b = el('button', { type: 'button', class: cls }, label);
        b.disabled = disabled || info.busy || selected === 0; b.addEventListener('click', onClick); return b;
      };
      const at = tracks.findIndex((t) => t.number === info.picked[kind]);
      box.append(el('div', { class: 'row order-tools' },
        el('span', { class: 'muted' }, `All ${selected} ticked file${selected === 1 ? '' : 's'}, tracks matched by language:`),
        wide('Preview this order…', () => previewForAll(kind, 'order'), !orderChanged(kind) && !info.defaultFirst[kind]),
        wide('Preview removing this language…', () => previewForAll(kind, 'delete'), at < 0, 'danger')));
    }
    if (movable && editable) box.append(keepRow(kind, tracks));
    if (movable && editable) {
      const picked = tracks.find((t) => t.number === info.picked[kind]);
      if (picked) box.append(trackEditor(kind, picked, file.container === 'Matroska'));
    }
    box.append(el('div', { class: 'table-wrap' }, table));
  }
  if (!editable) box.append(el('p', { class: 'muted' }, 'AVI files have no track settings, so their order cannot be set here.'));
}

// What the Edit dialog's arrangement of one file means for the other selected files: the
// languages in the order they are in here, and removing the language of the picked track.
// Both are shown as a preview first, in the results, where they can be applied.
async function previewForAll(kind, what) {
  const list = info.order[kind];
  const r = { ...commonRequest(true), kind: kind === 'audio' ? 'audio' : 'sub', in_place: false };
  if (!r.paths.length) { $('info-status').textContent = 'No files are ticked in the list.'; return; }
  if (what === 'order') {
    r.tasks = [{ name: 'set', what: 'order' }];
    r.order_lang = [...new Set(list.map((t) => t.language))];
    r.default_first = info.defaultFirst[kind];
  } else if (what === 'keep') {
    r.tasks = [{ name: 'preserve', what: 'lang' }];
    r.lang = info.keep[kind];
  } else {
    const track = list.find((t) => t.number === info.picked[kind]);
    if (!track) return;
    r.tasks = [{ name: 'delete', what: 'lang' }];
    r.lang = track.language;
  }
  $('info').close();
  await go(true, r);
}

// Rename the file within its folder, then keep the list (and what is ticked) pointing at it.
async function renameFile(event) {
  event.preventDefault();
  const file = info.file;
  const name = $('info-name').value.trim();
  if (!file || info.busy) return;
  try {
    const log = $('use-log').checked ? $('log').value.trim() : '';
    const renamed = await invoke('rename_file', { path: file.path, name, log });
    const path = renamed.path;
    const was = file.path;
    file.path = path;
    file.name = file.name.replace(/[^\\/]*$/, baseName(path));  // keep the folder part shown for a --dir scan
    for (const set of [state.selected, state.expanded, state.removed]) {
      if (set.has(was)) { set.delete(was); set.add(path); }
    }
    state.paths = state.paths.map((p) => (p === was ? path : p));
    $('info-title').textContent = file.name;
    $('info-path').textContent = path;
    $('info-name').value = baseName(path);
    const kept = renamed.log_entries
      ? `${renamed.log_entries} entr${renamed.log_entries === 1 ? 'y' : 'ies'} in the log now ${renamed.log_entries === 1 ? 'names' : 'name'} the new file, so Undo still works. `
      : '';
    $('info-status').textContent = `Renamed. ${kept}${renamed.log_error ? `${renamed.log_error}. ` : ''}`
      + 'A torrent client seeding this file will no longer find it, and another log that names it will not find it either.';
    renderFiles();
  } catch (error) {
    $('info-status').textContent = `Not renamed: ${error}`;
  }
}

// Run one change on the file in the dialog, after asking, then read the file again so the
// dialog and the list show what it is now.
async function changeFile(request, question) {
  const file = info.file;
  if (!(await confirmApply(question))) return null;
  let done = null;
  info.busy = true; setBusy(true); renderOrder();
  $('info-status').textContent = 'Working…';
  try {
    const summary = await invoke('run', { request: {
      paths: [file.path], dryrun: false,
      backup: $('backup').checked, force: $('force').checked,
      log: $('use-log').checked ? $('log').value.trim() : '', ...request } });
    const r = summary.results[0];
    const lines = r && r.changes.length ? r.changes.join('; ') : (r && r.message) || '';
    $('info-status').textContent = r ? `${statusLabel(r)}${lines ? `: ${lines}` : ''}` : 'Nothing was done.';
    done = r;
    const scanned = await invoke('scan', { request: { kind: 'audio', paths: [file.path] } });
    const fresh = scanned.results.find((x) => x.action === 'info' && x.data);
    if (fresh) {
      file.tracks = fresh.data.tracks; file.container = fresh.data.container;
      resetOrder(file);
      renderFiles();
    }
  } catch (error) {
    $('info-status').textContent = String(error);
  } finally {
    info.busy = false; setBusy(false); renderOrder();
  }
  return done;
}

// Adding a subtitle file as a new track (MKV only).
function showSubFile() {
  $('add-sub-file').textContent = info.subFile ? baseName(info.subFile) : 'None chosen (.srt, .ass, .ssa, .vtt or .sup)';
  $('add-sub-file').title = info.subFile;
}

async function chooseSubFile() {
  const chosen = await open({ multiple: false, filters: [
    { name: 'Subtitle files', extensions: ['srt', 'ass', 'ssa', 'vtt', 'sup'] }] });
  if (!chosen) return;
  info.subFile = Array.isArray(chosen) ? chosen[0] : chosen;
  showSubFile();
  if (!$('add-sub-lang').value) {  // a guess from the file name: movie.ita.srt, movie.Italian.srt
    const parts = baseName(info.subFile).toLowerCase().split('.');
    const hit = state.languages.find((l) => parts.includes(l.code) || parts.includes(l.name.toLowerCase()));
    if (hit) $('add-sub-lang').value = hit.code;
  }
}

async function addSubtitle() {
  const lang = $('add-sub-lang').value;
  if (!info.subFile) { $('info-status').textContent = 'Choose a subtitle file first.'; return; }
  if (!lang) { $('info-status').textContent = 'Choose the language of the subtitle.'; return; }
  const name = $('add-sub-name').value.trim();
  const language = state.languages.find((l) => l.code === lang);
  const r = await changeFile({ kind: 'sub', tasks: [{ name: 'add', what: 'subtitle' }],
    sub_file: info.subFile, lang, name: name || null },
  `Add ${baseName(info.subFile)} to ${info.file.name} as a subtitle track in ${language ? language.name : lang}`
    + `${name ? `, named "${name}"` : ''}? The file is rewritten.`);
  if (r && r.status === 'changed') { info.subFile = ''; $('add-sub-name').value = ''; showSubFile(); }
}

const kindWord = (kind) => (kind === 'audio' ? 'audio' : 'subtitle');

function applyOrder(kind) {
  const list = info.order[kind];
  return changeFile({ kind: kind === 'audio' ? 'audio' : 'sub', tasks: [{ name: 'set', what: 'order' }],
    order: list.map((t) => t.number), default_first: info.defaultFirst[kind] },
  `Rewrite ${info.file.name} with the ${kindWord(kind)} tracks in this order: `
    + `${list.map((t) => t.language + (t.name ? ` (${t.name})` : '')).join(', ')}`
    + `${info.defaultFirst[kind] ? ', and the first one the default' : ''}?`);
}

function deleteTrack(kind) {
  const track = info.order[kind].find((t) => t.number === info.picked[kind]);
  if (!track) return;
  return changeFile({ kind: kind === 'audio' ? 'audio' : 'sub', tasks: [{ name: 'delete', what: 'track' }],
    track: `${kind}:${track.number}` },
  `Remove ${kindWord(kind)} track ${track.number} (${track.language}${track.name ? `, ${track.name}` : ''}) `
    + `from ${info.file.name}? The file is rewritten without it.`);
}

async function showInfo(file) {
  const counts = ['video', 'audio', 'subtitles']
    .map((k) => [k, file.tracks.filter((t) => t.type === k).length]).filter(([, n]) => n)
    .map(([k, n]) => `${n} ${k}`).join(', ');
  $('info-title').textContent = file.name;
  $('info-path').textContent = file.path;
  $('info-name').value = baseName(file.path);
  const facts = $('info-facts');
  const fact = (term, value) => facts.append(el('dt', {}, term), el('dd', {}, value));
  facts.replaceChildren();
  fact('Container', file.container);
  fact('Tracks', counts || 'none');
  resetOrder(file);
  renderOrder();
  info.subFile = ''; showSubFile();
  $('add-sub-name').value = '';
  $('info-add').hidden = file.container !== 'Matroska' && file.container !== 'MP4';
  $('info-add').open = false;
  $('info-status').textContent = '';
  $('info-report').textContent = 'Reading…';
  $('info-report-box').open = false;
  $('info').showModal();
  try {
    const d = await invoke('file_details', { path: file.path });
    fact('Size', sizeText(d.size));
    if (d.modified) fact('Modified', new Date(d.modified * 1000).toLocaleString());
    $('info-report').textContent = d.report || `No report: ${d.report_error || 'unknown reason'}`;
  } catch (error) {
    $('info-report').textContent = `Could not read the file: ${error}`;
  }
}

function describeSource() {
  const usable = state.files.filter((f) => !f.error).length;
  $('source').textContent = (state.dir ? `Folder: ${state.dir}` : 'Chosen files')
    + `: ${usable} media file${usable === 1 ? '' : 's'} found.`;
}

// Take the ticked files off the list. Nothing happens to the files on disk.
function clearSelected() {
  const gone = new Set(state.files.filter((f) => !f.error && state.selected.has(f.path)).map((f) => f.path));
  if (!gone.size) return;
  for (const path of gone) state.removed.add(path);
  state.paths = state.paths.filter((p) => !gone.has(p));
  state.files = state.files.filter((f) => !gone.has(f.path));
  state.selected = new Set([...state.selected].filter((p) => !gone.has(p)));
  for (const path of gone) state.expanded.delete(path);
  if (state.files.length) describeSource();
  else $('source').textContent = 'The list is empty. Add a folder or files to start again.';
  renderFiles();
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
    state.files = summary.results.filter((x) => x.action === 'info' && !state.removed.has(x.file)).map((x) => ({
      path: x.file,
      name: prefix && slashes(x.file).startsWith(prefix)
        ? slashes(x.file).slice(prefix.length) : baseName(x.file),
      container: x.data ? x.data.container : '',
      tracks: x.data ? x.data.tracks : [],
      error: x.status === 'error' ? x.message : '',
    }));
    state.selected = new Set(state.files.filter((f) => !f.error).map((f) => f.path));
    state.expanded.clear();
    describeSource();
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
  state.dir = chosen; state.paths = []; state.removed.clear();
  await scan();
}

async function addFiles() {
  const chosen = await open({ multiple: true, filters: [
    { name: 'Media files', extensions: ['mkv', 'mp4', 'm4v', 'mov', 'avi', 'webm'] }] });
  if (!chosen || !chosen.length) return;
  state.paths = Array.isArray(chosen) ? chosen : [chosen]; state.dir = ''; state.removed.clear();
  await scan();
}

// Dropping files or a folder on the window -------------------------------------------------

async function handleDrop(paths) {
  if (state.running || document.querySelector('dialog[open]')) return;
  const found = await invoke('classify_dropped', { paths });
  if (found.folder) {
    state.dir = found.folder; state.paths = []; state.removed.clear();
    await scan();
  } else if (found.files.length) {
    state.paths = found.files; state.dir = ''; state.removed.clear();
    await scan();
  } else {
    $('source').textContent = 'Nothing to add: drop MKV, MP4 (M4V, MOV) or AVI files, or a folder of them.';
    return;
  }
  if (found.ignored) $('source').textContent += found.ignored === 1 ? ' 1 dropped file was not a media file.' : ` ${found.ignored} dropped files were not media files.`;
}

async function enableDrop() {
  try {
    await window.__TAURI__.webview.getCurrentWebview().onDragDropEvent((event) => {
      const kind = event.payload.type;
      document.body.classList.toggle('dropping', kind === 'enter' || kind === 'over');
      if (kind === 'drop') handleDrop(event.payload.paths).catch((error) => { $('source').textContent = String(error); });
    });
  } catch (error) {
    // dropping is a convenience: the buttons still work without it
  }
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
  $('clear').disabled = state.running || !chosen;
  $('add-subs').disabled = state.running || !mkvTicked().length;
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

// One track as a short text for the before and after tables: language, * default, forced, name.
function trackText(t) {
  if (!t) return '(none)';
  return `${t.language}${t.default ? '*' : ''}${t.forced ? ' (forced)' : ''}${t.name ? ` "${t.name}"` : ''}`;
}

const sameTrack = (a, b) => Boolean(a) && Boolean(b) && a.language === b.language && a.default === b.default
  && a.forced === b.forced && a.name === b.name;

// The audio and subtitle tracks of a file before and after a change, side by side by place.
function layoutView(layout, open) {
  const box = el('div', { class: 'layout' });
  for (const kind of ['audio', 'subtitles']) {
    const before = layout.before.filter((t) => t.type === kind);
    const after = layout.after.filter((t) => t.type === kind);
    const count = Math.max(before.length, after.length);
    if (!count) continue;
    const rows = [];
    for (let i = 0; i < count; i += 1) {
      const changed = !sameTrack(before[i], after[i]);
      rows.push(el('tr', {}, el('td', {}, i + 1), el('td', {}, trackText(before[i])),
        el('td', { class: changed ? 'diff' : '' }, trackText(after[i]))));
    }
    box.append(el('table', { class: 'layout-table' },
      el('thead', {}, el('tr', {}, el('th', {}, KIND_TITLES[kind]), el('th', {}, 'Before'), el('th', {}, 'After'))),
      el('tbody', {}, ...rows)));
  }
  return el('details', open ? { open: '' } : {}, el('summary', {}, 'Before and after (* is default)'), box);
}

// The files being copied or rewritten right now, each with how far it has got.
function showProgress(p) {
  const box = $('file-progress');
  let row = [...box.children].find((c) => c.dataset.file === p.file);
  if (!row) {
    row = el('div', { class: 'file-progress' }, el('span', { class: 'label' }), el('progress', { max: '1', value: '0' }),
      el('span', { class: 'percent muted' }));
    row.dataset.file = p.file;
    box.append(row);
  }
  const fraction = p.total > 0 ? Math.min(1, p.done / p.total) : 0;
  row.querySelector('.label').textContent = `${baseName(p.file)}: ${p.stage.toLowerCase()}…`;
  row.querySelector('progress').value = fraction;
  row.querySelector('.percent').textContent = `${Math.round(fraction * 100)}%`;
}

function clearProgress(file) {
  const box = $('file-progress');
  for (const row of [...box.children]) if (file === undefined || row.dataset.file === file) row.remove();
}

function addResult(r) {
  if (r.action === 'check') return;
  clearProgress(r.file);
  const lines = r.changes.length ? r.changes : (r.message ? [r.message] : []);
  const item = el('li', { class: `s-${r.status}` },
    el('div', {}, el('span', { class: 'status' }, statusLabel(r)),
      el('span', { class: 'file' }, baseName(r.file))),
    ...(lines.length ? [el('ul', { class: 'lines' }, ...lines.map((l) => el('li', {}, l)))] : []),
    ...(r.status === 'changed' && r.data && r.data.layout ? [layoutView(r.data.layout, r.dry_run)] : []));
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
  await goMany([req || request(dry)], dry);
}

// Run one request after another (a change that is not the same for every file, such as
// subtitles each with its own file, is one request per file), showing them as one run.
async function goMany(requests, dry) {
  const total = requests.reduce((n, r) => n + r.paths.length, 0);
  if (!total && !requests.some((r) => r.tasks.some((t) => t.name === 'undo'))) return;
  $('results-card').hidden = false;
  $('results').replaceChildren();
  clearProgress();
  $('warnings').hidden = true; $('warnings').textContent = '';
  $('apply-preview').hidden = true;
  $('summary').textContent = dry ? 'Previewing…' : 'Working…';
  const bar = $('progress');
  bar.max = Math.max(1, total); bar.value = 0; bar.hidden = total === 0;
  $('results-card').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  setBusy(true);
  try {
    const results = [];
    let code = 0;
    for (const r of requests) {
      const summary = await invoke('run', { request: r });
      results.push(...summary.results);
      code = summary.code;
      if (code === 130) break;
    }
    const changed = showSummary(results, dry);
    if (code === 130) $('summary').textContent += ' (stopped)';
    if (dry && changed > 0) {
      state.again = requests.map((r) => ({ ...r, dryrun: false }));
      $('apply-preview').hidden = false;
    }
    if (!dry) await scan();
  } catch (error) {
    $('summary').textContent = String(error);
  } finally {
    bar.hidden = true;
    clearProgress();
    setBusy(false);
  }
}

async function apply(req) {
  const requests = Array.isArray(req) ? req : [req || request(false)];
  const paths = requests.reduce((n, r) => n + r.paths.length, 0);
  const what = requests.some((r) => r.tasks.some((t) => t.name === 'undo'))
    ? 'Undo the changes recorded in the log?'
    : `Change ${paths} file${paths === 1 ? '' : 's'}?`;
  if (!(await confirmApply(what))) return;
  await goMany(requests, false);
}

async function undo() {
  const chosen = await open({ multiple: false, filters: [
    { name: 'fml logs', extensions: ['jsonl', 'log', 'json', 'txt'] }, { name: 'All files', extensions: ['*'] }] });
  if (!chosen) return;
  await go(true, { tasks: [{ name: 'undo', what: chosen }], dryrun: true, paths: [] });
}

// Adding subtitle files to many files at once: matched by name, checked by hand ---------

const subs = { files: [], rows: [] };

const mkvTicked = () => state.files.filter((f) => !f.error && state.selected.has(f.path)
  && (f.container === 'Matroska' || f.container === 'MP4'));

function openSubs() {
  subs.files = []; subs.rows = [];
  $('subs-unmatched').hidden = true; $('subs-status').textContent = '';
  renderSubs();
  $('subs').showModal();
}

async function chooseSubs() {
  const chosen = await open({ multiple: true, filters: [
    { name: 'Subtitle files', extensions: ['srt', 'ass', 'ssa', 'vtt', 'sup'] }] });
  if (!chosen || !chosen.length) return;
  await addSubFiles(Array.isArray(chosen) ? chosen : [chosen]);
}

async function addSubFiles(paths) {
  subs.files = [...new Set([...subs.files, ...paths])];
  const found = await invoke('match_subtitle_files', { media: mkvTicked().map((f) => f.path), subs: subs.files });
  const before = new Map(subs.rows.map((r) => [r.file, r]));
  subs.rows = found.matches.map((m) => before.get(m.file)
    || { file: m.file, media: m.media, lang: m.lang || '', name: m.name, forced: m.forced, include: Boolean(m.lang) });
  const list = $('subs-unmatched');
  list.hidden = found.unmatched.length === 0;
  list.textContent = found.unmatched.length
    ? `No ticked MKV or MP4 file matches: ${found.unmatched.map(baseName).join(', ')}` : '';
  renderSubs();
}

function renderSubs() {
  const body = $('subs-table').querySelector('tbody');
  body.replaceChildren();
  for (const row of subs.rows) {
    const media = state.files.find((f) => f.path === row.media);
    const include = el('input', { type: 'checkbox', 'aria-label': `Add ${baseName(row.file)}` });
    include.checked = row.include;
    include.addEventListener('change', () => { row.include = include.checked; renderSubs(); });
    const lang = el('select', { 'aria-label': `Language of ${baseName(row.file)}` },
      el('option', { value: '' }, '(choose)'),
      ...state.languages.map((l) => el('option', { value: l.code }, l.name)));
    lang.value = row.lang;
    lang.addEventListener('change', () => { row.lang = lang.value; if (row.lang) row.include = true; renderSubs(); });
    const name = el('input', { type: 'text', size: '14', 'aria-label': `Name of ${baseName(row.file)}` });
    name.value = row.name;
    name.addEventListener('input', () => { row.name = name.value; });
    const forced = el('input', { type: 'checkbox', 'aria-label': `${baseName(row.file)} is forced` });
    forced.checked = row.forced;
    forced.addEventListener('change', () => { row.forced = forced.checked; });
    body.append(el('tr', {}, el('td', { class: 'narrow' }, include),
      el('td', { class: 'name' }, media ? media.name : baseName(row.media)),
      el('td', { class: 'name' }, baseName(row.file)), el('td', {}, lang), el('td', {}, name), el('td', {}, forced)));
  }
  const chosen = subs.rows.filter((r) => r.include);
  const missing = chosen.filter((r) => !r.lang).length;
  const files = new Set(chosen.map((r) => r.media)).size;
  $('subs-count').textContent = subs.files.length
    ? `${subs.rows.length} of ${subs.files.length} matched; ${chosen.length} to add to ${files} file${files === 1 ? '' : 's'}`
    : `${mkvTicked().length} ticked MKV or MP4 file${mkvTicked().length === 1 ? '' : 's'}`;
  $('subs-status').textContent = missing ? `Choose a language for ${missing} subtitle${missing === 1 ? '' : 's'}, or untick ${missing === 1 ? 'it' : 'them'}.` : '';
  for (const id of ['subs-preview', 'subs-apply']) $(id).disabled = !chosen.length || missing > 0 || state.running;
}

// One request per media file, each with all its subtitles.
function subtitleRequests(dry) {
  const byMedia = new Map();
  for (const row of subs.rows.filter((r) => r.include && r.lang)) {
    if (!byMedia.has(row.media)) byMedia.set(row.media, []);
    byMedia.get(row.media).push({ file: row.file, lang: row.lang, name: row.name.trim(), forced: row.forced });
  }
  return [...byMedia].map(([media, list]) => ({
    ...commonRequest(dry), paths: [media], kind: 'sub', in_place: false,
    tasks: [{ name: 'add', what: 'subtitle' }], subtitles: list }));
}

async function runSubs(dry) {
  const requests = subtitleRequests(dry);
  if (!requests.length) return;
  if (!dry) {
    const n = requests.reduce((t, r) => t + r.subtitles.length, 0);
    if (!(await confirmApply(`Add ${n} subtitle file${n === 1 ? '' : 's'} to ${requests.length} file${requests.length === 1 ? '' : 's'}? Each file is rewritten once.`))) return;
  }
  $('subs').close();
  await goMany(requests, dry);
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
  $('add-sub-lang').append(el('option', { value: '' }, '(choose)'),
    ...state.languages.map((l) => el('option', { value: l.code }, l.name)));
  updateForm();
  updateButtons();

  await listen('fml-result', (event) => addResult(event.payload));
  await listen('fml-progress', (event) => showProgress(event.payload));
  await enableDrop();
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
$('clear').addEventListener('click', clearSelected);
$('add-subs').addEventListener('click', openSubs);
$('subs-choose').addEventListener('click', chooseSubs);
$('subs-close').addEventListener('click', () => $('subs').close());
$('subs-preview').addEventListener('click', () => runSubs(true));
$('subs-apply').addEventListener('click', () => runSubs(false));
$('select-all').addEventListener('change', (e) => {
  state.selected = new Set(e.target.checked ? state.files.filter((f) => !f.error).map((f) => f.path) : []);
  renderFiles();
});
$('preview').addEventListener('click', () => go(true));
$('apply').addEventListener('click', () => apply());
$('apply-preview').addEventListener('click', () => apply(state.again));
$('undo').addEventListener('click', undo);
$('cancel').addEventListener('click', () => invoke('cancel'));
$('info-rename').addEventListener('submit', renameFile);
$('add-sub-choose').addEventListener('click', chooseSubFile);
$('add-sub-go').addEventListener('click', addSubtitle);
$('info-close').addEventListener('click', () => $('info').close());

start().catch((error) => { $('source').textContent = `Could not start: ${error}`; });
