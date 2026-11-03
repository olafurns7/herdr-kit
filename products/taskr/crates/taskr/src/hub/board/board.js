// The hub board: one fetch of /api/board a minute; every string goes in as text.
'use strict';
const COLS = [['not_done', 'Not done'], ['in_progress', 'In progress'], ['done', 'Done']];
const REPO = /^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/;
const prClass = s => ({clean: 'ok', has_hooks: 'ok', merged: 'ok', unstable: 'warn', behind: 'warn', blocked: 'warn', dirty: 'bad', closed: 'mut', draft: 'mut'})[s] || 'mut';
const checkClass = s => ({success: 'ok', neutral: 'ok', skipped: 'ok', failure: 'bad', error: 'bad', cancelled: 'bad', timed_out: 'bad', action_required: 'bad'})[s] || 'warn';
const $ = id => document.getElementById(id);

function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) k === 'class' ? e.className = v : e.setAttribute(k, v);
  for (const k of kids) if (k != null && k !== '') e.append(k);
  return e;
}
const span = (cls, text) => el('span', {class: cls}, String(text));
const blocked = t => t.blocked && t.column !== 'done';
const groupName = g => `${g.campaign || 'root ' + g.root} · ${g.phase || 'untagged'}`;

function card(t, label) {
  const c = el('article', {class: 'card' + (blocked(t) ? ' blocked' : '')});
  c.append(el('div', {class: 't'}, span('id', [t.key, t.ref].filter(Boolean).join(' ')), t.title));
  const m = el('div', {class: 'meta'});
  if (blocked(t)) m.append(span('bad', 'Blocked: ' + t.blocked));
  if (t.lanes) m.append(span('', 'lane ' + t.lanes));
  if (t.pr !== '') {
    const num = '#' + t.pr;
    const link = REPO.test(t.pr_repo) ? el('a', {href: `https://github.com/${t.pr_repo}/pull/${Number(t.pr)}`, target: '_blank', rel: 'noopener noreferrer'}, num) : num;
    const p = el('span', {}, link, ' ', t.pr_state ? span(prClass(t.pr_state), t.pr_state) : span('mut', 'no state yet'));
    for (const [name, value] of Object.entries(t.checks || {})) p.append(`, ${name} at head `, span(checkClass(value), value));
    if (t.queue !== '') p.append(', queue ' + t.queue);
    m.append(p);
  }
  if (t.waits_open) m.append(span('', 'waits ' + t.waits_open.split(' ').map(label).join(', ')));
  if (t.waits_text) m.append(span('', 'waits: ' + t.waits_text));
  if (t.asks) m.append(span('warn', 'ask ' + t.asks + ' open'));
  if (t.note) m.append(span('', t.note));
  if (m.childNodes.length) c.append(m);
  return c;
}

function draw(d) {
  const tickets = d.tickets, groups = d.groups, asks = d.asks, alarms = d.alarms;
  const byKey = new Map(tickets.map(t => [t.key, t]));
  const label = k => (byKey.get(k) || {}).ref || k;
  const ua = $('s-asks'); ua.textContent = '';
  for (const a of asks) ua.append(el('li', {}, span(a.blocking ? 'bad' : 'warn', (a.blocking ? 'BLOCKING ' : '') + a.ask), ' ', a.campaign ? span('mut', a.campaign + ' · ') : '', a.text));
  if (!asks.length) ua.append(el('li', {class: 'mut'}, 'No open owner asks.'));
  const ul = $('s-alarms'); ul.textContent = '';
  for (const a of alarms) ul.append(el('li', {}, span('mut', a.time + ' '), span(a.event === 'HUB-RED' || /FAIL|STUCK/.test(a.event) ? 'bad' : 'warn', a.event), ' ', a.detail));
  if (!alarms.length) ul.append(el('li', {class: 'mut'}, 'No alarms.'));
  $('s-at').textContent = String(d.at).slice(11, 16) + 'Z';
  const n = col => tickets.filter(t => t.column === col).length;
  const blockedN = tickets.filter(blocked).length;
  $('lede').textContent = tickets.length
    ? `${n('in_progress')} tickets in progress, ${n('not_done')} not done, ${n('done')} done${blockedN ? ', ' + blockedN + ' blocked' : ''}; ${asks.length || 'no'} owner asks open.`
    : 'No tickets.';
  const host = $('groups'); host.textContent = '';
  const tb = document.querySelector('#totals tbody'); tb.textContent = '';
  for (const g of groups) {
    const ts = tickets.filter(t => t.group === g.key);
    const sec = el('section', {class: 'panel' + (g.parked ? ' waiting' : '')}, el('h2', {}, groupName(g)));
    if (g.parked) sec.append(el('div', {class: 'waitline'}, 'Parked: ' + g.parked));
    const cols = el('div', {class: 'cols'});
    const counts = [];
    for (const [k, name] of COLS) {
      const inCol = ts.filter(t => t.column === k);
      counts.push(inCol.length);
      const list = el('div', {class: 'list'});
      for (const t of inCol) list.append(card(t, label));
      if (!inCol.length) list.append(span('mut', 'None.'));
      cols.append(el('div', {class: 'col'}, el('h3', {}, `${name} (${inCol.length})`), list));
    }
    sec.append(cols);
    host.append(sec);
    tb.append(el('tr', {}, el('td', {}, groupName(g)), ...counts.map(c => el('td', {}, String(c))), el('td', {}, String(ts.filter(blocked).length))));
  }
  const chain = $('chain'); chain.textContent = '';
  for (const t of tickets.filter(t => t.column !== 'done' && (t.waits || t.waits_text || t.blocked)).sort((a, b) => a.root - b.root || a.ord - b.ord)) {
    const li = el('li', {}, span('id', label(t.key) + ' '), t.title);
    const waits = t.waits ? t.waits.split(' ') : [];
    if (waits.length) {
      li.append(' — waits ');
      waits.forEach((w, i) => { if (i) li.append(', '); li.append((byKey.get(w) || {}).column === 'done' ? el('s', {}, label(w)) : label(w)); });
    }
    if (t.waits_text) li.append('; ' + t.waits_text);
    if (t.blocked) li.append(' ', span('bad', 'blocked: ' + t.blocked));
    chain.append(li);
  }
}

async function load() {
  try {
    const r = await fetch('/api/board', {cache: 'no-store'});
    if (!r.ok) throw new Error('HTTP ' + r.status);
    draw(await r.json());
    $('s-load').textContent = 'Loaded ' + new Date().toISOString().slice(11, 16) + 'Z; next in a minute.';
  } catch (e) {
    $('s-load').textContent = `The board could not be read (${e.message}); trying again in a minute.`;
    if ($('lede').textContent === 'Loading…') $('lede').textContent = 'No data yet.';
  }
}
load();
setInterval(load, 60000);
