// The frame every drawing of Away shares, so a change to the top bar, the rail or
// the tabs is made once. Load it at the end of a drawing's page, after icons.js
// and ds.js:
//
//   <div class="computer" data-view="home">…</div>
//       the frame on a computer: the top bar with the mark >a, the search and the
//       avatar, and the rail of icons down the left with "home" current and settings
//       at its foot. The drawing's content becomes the page beside the rail, and what
//       it places by coordinates is placed against that page.
//       data-role="admin" adds the administration view to the rail (and to a phone's
//       tabs), as an administrator or a reader of the records has it.
//       data-count="2" shows two open sessions on the sessions icon.
//       data-query="fin" shows typed text in the search field.
//       data-bar="none" draws no frame: a session has the whole window.
//   <div class="phone" data-view="home">…</div>
//       the top bar (the name, the search, the avatar) and the tabs, icons alone.
//       data-bar="back" data-title="settings" for a back arrow and a title;
//       data-bar="none" for no top bar; data-tabs="none" for no tabs.
//   <div class="phone side" …> the phone held sideways, 780 × 360: the same bar,
//       slimmer, and the tabs as a rail down the left edge.
//   data-view="none" marks no view as current (settings: data-view="settings").
//
//   <div class="computer" data-bar="none"><div class="session" data-sbar="closed">…</div></div>
//       a session, which has the whole window: the handle that opens its bar.
//       data-sbar="open" draws the bar: data-tabs="desktop:fin-app-02,shell:build-01"
//       are the open sessions (desktop, programs or shell, then the machine's name),
//       the first one current unless data-current names another; data-as="as alex.urban"
//       says who one is on the machine; data-on="keys" shows a button pressed, its
//       menu open (keys, clipboard, picture, look, find, fill, end; on a phone kb, more).
//       The machine's picture is a <div class="remote …"> of remote.js.
//
//   <div class="prail" data-lang="DE" data-time="14:32" data-day="16.11.">…windows…</div><div class="warea">…</div>
//       in a session with programs: the rail down the left, as a taskbar, and the room the windows
//       have beside it, against which they are placed. A drawing writes the windows, each as its
//       icon: <span class="pr-win on" data-p="L" title="Ledger – November"></span>, on being the
//       one in front; data-p is the program's icon, as for a window of remote.js. The Away mark at
//       the top, the controls and the foot (the button of the tray where data-tray is set, the
//       keyboard and the time) are drawn here. data-on="launch" shows the mark pressed, its list
//       of programs open; data-on="tray" the button of the tray. On a phone the rail lies over
//       the picture (class="prail over").
//
// A dialog (<ds-modal>), the avatar's menu (class usermenu) and anything marked
// data-over lie over the whole frame, the bar and the rail too, and are placed
// against it.
//
// data-co-parts="logo:1:top,rail:2,search:3:top" on a frame numbers the parts
// frame.js draws itself: logo, rail, search, avatar, tabs, back, settings, sessions,
// admin, current (the current view); and of a session's bar: handle, away (the mark),
// stabs (the open sessions), stab (the current one), add, as, keys, clipboard, picture,
// look, find, fill, endsession, closebar, kb, more; and of a program's rail: pmark (the mark),
// pkeys, pclip, pfill, pend, ptray (the button of the tray), plang (the keyboard), pclock
// (name:number:side).
//
// data-co-in=".nc-illu__title 3, .nc-btn 4 right" numbers parts inside an element by a
// selector (the parts a <ds-*> tag draws itself).
//
// data-co="2" on any element inside a frame numbers that part of the drawing; the
// text under the figure says what each number is. The number sits in the margin
// left of the screen, level with the element, with a line to it;
// data-co-side="right" puts it in the right margin, "top" and "bottom" above and
// below. Two numbers on one side at the same height push each other down, so number
// things at different heights, or put one on the right.
(function () {
  // the design system's components first: <ds-*> tags become the package's markup (ds.js)
  if (window.DS) DS.expand(document.body);
  else console.error('load ds.js before frame.js');
  const icon = (name, cls = '') => `<i data-i="${name}" class="${cls}"></i>`;
  const chevron = window.DS ? DS.svg.chevron : '';
  const logo = `<span class="logo">${chevron}away</span>`;
  const make = (html) => { const t = document.createElement('template'); t.innerHTML = html.trim(); return t.content.firstChild; };
  // the views, in their order, each with its icon; administration only for those who have it
  const views = (f) => [['home', 'house'], ['programs', 'app-window'], ['machines', 'monitor'], ['sessions', 'layers'], ...(f.dataset.role ? [['administration', 'shield']] : [])];
  const count = (f, v) => (v === 'sessions' && f.dataset.count && f.dataset.count !== '0' ? `<span class="count">${f.dataset.count}</span>` : '');
  const over = (f, from) => { for (const m of from.querySelectorAll('.nc-modal, .usermenu, [data-over]')) f.appendChild(m); };

  for (const f of document.querySelectorAll('.computer')) {
    if (f.dataset.bar === 'none') continue;
    const view = f.dataset.view || 'home';
    const q = f.dataset.query;
    const affix = window.DS ? `<span class="nc-input__affix nc-input__affix--prefix">${DS.svg.searchAffix}</span>` : '';
    const field = `<div class="nc-input nc-input--md search-field${q ? ' is-focused' : ''}">${affix}<input class="nc-input__control" placeholder="search machines…" value="${q || ''}"></div>`;
    const page = document.createElement('div');
    page.className = 'cpage';
    while (f.firstChild) page.appendChild(f.firstChild);
    const it = (v, i) => `<span class="it it-${v}${v === view ? ' on' : ''}" aria-label="${v}" title="${v}">${icon(i)}${count(f, v)}</span>`;
    f.classList.add('railed');
    f.append(make(`<header class="tb"><span class="mark logo" aria-label="away, home">${chevron}<b>a</b></span><div class="end">${field}<span class="av">au</span></div></header>`));
    f.append(make(`<nav class="crail" aria-label="views">${views(f).map(([v, i]) => it(v, i)).join('')}<span class="gap"></span>${it('settings', 'settings')}</nav>`));
    f.append(page);
    over(f, page);
  }

  for (const f of document.querySelectorAll('.phone')) {
    const view = f.dataset.view || 'home';
    const side = f.classList.contains('side'); // held sideways: 780 × 360, the tabs become a rail on the left
    const scroll = document.createElement('div');
    scroll.className = 'scroll';
    while (f.firstChild) scroll.appendChild(f.firstChild);
    const col = side ? document.createElement('div') : f;
    if (side) { col.className = 'side-col'; f.appendChild(col); }
    col.appendChild(scroll);
    if (f.dataset.bar === 'back') {
      col.prepend(make(`<header class="tb back"><span class="icon-b">${icon('arrow-left')}</span><span class="title">${f.dataset.title || ''}</span></header>`));
    } else if (f.dataset.bar !== 'none') {
      col.prepend(make(`<header class="tb">${logo}<span class="icon-b search">${icon('search')}</span><span class="av">au</span></header>`));
    }
    over(f, scroll);
    if (f.dataset.tabs !== 'none' && f.dataset.bar !== 'none') {
      // the tabs are icons alone; a view's name is for a screen reader
      const items = views(f).map(([v, i]) => `<span class="it-${v}${v === view ? ' on' : ''}" aria-label="${v}">${icon(i)}${count(f, v)}</span>`).join('');
      if (side) f.prepend(make(`<nav class="siderail tabs">${items}</nav>`));
      else f.appendChild(make(`<nav class="tabs">${items}</nav>`));
    }
  }

  // a session's bar, over the machine's picture; the handle that opens it while it is closed
  const KIND = { desktop: 'monitor', programs: 'app-window', shell: 'square-terminal' };
  const sbtn = (cls, name, i, on) => `<button type="button" aria-label="${name}" title="${name}" class="nc-iconbtn nc-iconbtn--ghost nc-iconbtn--md sb-${cls}${on === cls ? ' on' : ''}">${icon(i, 's18')}</button>`;
  for (const s of document.querySelectorAll('.session[data-sbar]')) {
    if (s.dataset.sbar === 'closed') { s.appendChild(make(`<span class="shandle" aria-label="open the session's bar">${icon('chevron-down')}</span>`)); continue; }
    const tabs = (s.dataset.tabs || '').split(',').map((t) => t.trim().split(':')).filter((t) => t.length === 2);
    const cur = s.dataset.current || (tabs[0] || [])[1];
    const kind = s.dataset.kind || (tabs.find(([, n]) => n === cur) || ['desktop'])[0];
    const on = s.dataset.on || '';
    const mark = `<span class="mark" aria-label="away: the machines and the open sessions">${chevron}<b>a</b></span>`;
    if (s.closest('.phone')) {
      // on a phone: the session's name opens the list of the open sessions; the keyboard; the rest in a menu
      s.appendChild(make(`<div class="sbar">${mark}<span class="stab on">${icon(KIND[kind])}${cur}${icon('chevron-down', 's14')}</span><span class="sp"></span>
        <div class="sgrp">${sbtn('kb', 'keyboard', 'keyboard', on)}${sbtn('more', 'more', 'ellipsis-vertical', on)}</div></div>`));
      continue;
    }
    // a shell has its text and its find; a desktop has its picture's size; a program's windows have their own
    const own = kind === 'shell' ? sbtn('look', 'text', 'a-large-small', on) + sbtn('find', 'find', 'text-search', on)
      : kind === 'programs' ? '' : sbtn('picture', 'picture', 'scaling', on);
    s.appendChild(make(`<div class="sbar">${mark}<div class="stabs">${tabs.map(([k, n]) => `<span class="stab${n === cur ? ' on' : ''}">${icon(KIND[k])}${n}</span>`).join('')}<span class="stab add" aria-label="open another machine">${icon('plus')}</span></div>
      <span class="sp"></span>${s.dataset.as ? `<span class="as">${s.dataset.as}</span>` : ''}
      <div class="sgrp">${sbtn('keys', 'keys', 'keyboard', on)}${sbtn('clipboard', 'clipboard', 'clipboard', on)}${own}${sbtn('fill', 'fill the screen', 'fullscreen', on)}</div>
      <div class="sgrp">${sbtn('end', 'end', 'power', on)}</div><div class="sgrp">${sbtn('close', 'close the bar', 'chevron-up', on)}</div></div>`));
  }

  // the rail of a session with programs: the windows are the drawing's; the mark at the top, the
  // controls and the foot with the keyboard and the time are drawn here
  for (const r of document.querySelectorAll('.prail')) {
    for (const w of r.querySelectorAll('.pr-win')) {
      const [letter, tone] = (w.dataset.p || '').split(':');
      w.setAttribute('aria-label', `bring to the front: ${w.title}`);
      w.innerHTML = `<span class="pico ${tone || ''}">${letter}</span>`;
    }
    const btn = (cls, name, i) => `<span class="pr-btn pr-${cls}" aria-label="${name}" title="${name}">${icon(i)}</span>`;
    r.prepend(make(`<span class="pr-top"><span class="pr-mark${r.dataset.on === 'launch' ? ' on' : ''}" aria-label="open a program">${chevron}<b>a</b></span></span>`));
    r.append(make('<span class="pr-gap"></span>'));
    // over the picture on a phone, the rail has a button that puts it away
    const close = r.classList.contains('over') ? [btn('close', 'close the bar', 'x')] : [];
    for (const b of [btn('keys', 'keys', 'keyboard'), btn('clip', 'clipboard', 'clipboard'), btn('fill', 'fill the screen', 'fullscreen'), btn('end', 'end', 'power'), ...close]) r.append(make(b));
    const tray = r.dataset.tray !== undefined ? `<span class="pr-btn pr-tray${r.dataset.on === 'tray' ? ' on' : ''}" aria-label="the tray" title="the tray">${icon('chevron-up')}</span>` : '';
    r.append(make(`<span class="pr-foot">${tray}<span class="pr-lang" title="the keyboard">${r.dataset.lang || 'DE'}</span>
      <span class="pr-clock" aria-label="the time on your device"><span>${r.dataset.time || '14:32'}</span><span class="day">${r.dataset.day || '16.11.'}</span></span></span>`));
  }

  for (const el of document.querySelectorAll('i[data-i]')) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('class', 'i ' + el.className);
    if (el.getAttribute('style')) svg.setAttribute('style', el.getAttribute('style'));
    for (const a of el.attributes) if (a.name.startsWith('data-co')) svg.setAttribute(a.name, a.value);
    svg.innerHTML = (window.ICONS || {})[el.dataset.i] || '';
    if (!window.ICONS?.[el.dataset.i]) console.error(`no icon "${el.dataset.i}" in icons.js`);
    el.replaceWith(svg);
  }

  const parts = { logo: '.tb .logo', rail: '.crail, .siderail', search: '.tb .search-field, .tb .search', avatar: '.tb .av', tabs: '.tabs',
                  back: '.tb.back .icon-b', settings: '.crail .it-settings', sessions: '.it-sessions', admin: '.it-administration',
                  current: '.crail .it.on, .tabs .on',
                  handle: '.shandle', away: '.sbar .mark', stabs: '.sbar .stabs', stab: '.sbar .stab.on', add: '.sbar .stab.add', as: '.sbar .as',
                  keys: '.sb-keys', clipboard: '.sb-clipboard', picture: '.sb-picture', look: '.sb-look', find: '.sb-find', fill: '.sb-fill',
                  endsession: '.sb-end', closebar: '.sb-close', kb: '.sb-kb', more: '.sb-more',
                  pmark: '.pr-mark', pkeys: '.pr-keys', pclip: '.pr-clip', pfill: '.pr-fill', pend: '.pr-end', ptray: '.pr-tray', plang: '.pr-lang',
                  pclock: '.pr-clock' };
  for (const f of document.querySelectorAll('[data-co-parts]')) {
    for (const entry of f.dataset.coParts.split(',')) {
      const [name, n, side] = entry.trim().split(':');
      const el = parts[name] && f.querySelector(parts[name]);
      if (!el) { console.error(`data-co-parts: no "${name}" in this frame`); continue; }
      el.dataset.co = n;
      if (side) el.dataset.coSide = side;
    }
  }

  // data-co-in=".nc-illu__title 3, .nc-btn 4 right" on any element numbers parts inside it by a selector,
  // such as the parts a <ds-*> tag draws itself (an empty state's title, a banner's button): each entry is
  // a selector, then the number, then optionally the side
  for (const host of document.querySelectorAll('[data-co-in]')) {
    for (const entry of host.dataset.coIn.split(',')) {
      const words = entry.trim().split(/\s+/);
      const side = ['left', 'right', 'top', 'bottom'].includes(words[words.length - 1]) ? words.pop() : '';
      const n = words.pop(), sel = words.join(' ');
      const el = sel && host.querySelector(sel);
      if (!el || !/^\d+$/.test(n || '')) { console.error(`data-co-in: no "${sel}" with a number inside`); continue; }
      el.dataset.co = n;
      if (side) el.dataset.coSide = side;
    }
  }

  // the numbers, once the fonts have set the text where it stays: in the margin
  // beside the screen, level with what they mark, with a line to it
  const place = () => {
    const used = new Map();
    for (const el of document.querySelectorAll('[data-co]')) {
      const frame = el.closest('.computer, .phone'), shot = el.closest('.shot');
      if (!frame || !shot) continue;
      shot.style.position = 'relative';
      const s = shot.getBoundingClientRect(), f = frame.getBoundingClientRect(), r = el.getBoundingClientRect();
      const side = ['right', 'top', 'bottom'].includes(el.dataset.coSide) ? el.dataset.coSide : 'left';
      const ys = used.get(frame) || { left: [], right: [], top: [], bottom: [] };
      used.set(frame, ys);
      if (side === 'top' || side === 'bottom') {
        // above or below the screen, over the middle of the part, with a line down or up to it
        let x = Math.round(r.left - s.left + Math.min(r.width, 60) / 2 - 11);
        while (ys[side].some((u) => Math.abs(u - x) < 26)) x += 26;
        ys[side].push(x);
        const y = Math.round(side === 'top' ? f.top - s.top - 26 : f.bottom - s.top + 4);
        const from = side === 'top' ? y + 22 : Math.round(Math.min(r.bottom, f.bottom - 16) - s.top + 6);
        const to = side === 'top' ? Math.round(Math.max(r.top, f.top + 16) - s.top - 6) : y;
        if (to - from > 2) shot.appendChild(make(`<i class="co-line ${side}" style="left:${x + 10}px;top:${from}px;height:${to - from}px"></i>`));
        shot.appendChild(make(`<b class="co" style="left:${x}px;top:${y}px">${el.dataset.co}</b>`));
        continue;
      }
      let y = Math.round(r.top - s.top + Math.min(r.height, 44) / 2 - 11);
      while (ys[side].some((u) => Math.abs(u - y) < 24)) y += 24;
      ys[side].push(y);
      const x = Math.round(side === 'left' ? f.left - s.left - 26 : f.right - s.left + 4);
      // a part cut off by the screen's edge is met where it is still in view
      const from = side === 'left' ? x + 22 : Math.round(Math.min(r.right, f.right - 16) - s.left + 6);
      const to = side === 'left' ? Math.round(Math.max(r.left, f.left + 16) - s.left - 6) : x;
      if (to - from > 2) shot.appendChild(make(`<i class="co-line ${side}" style="left:${from}px;top:${y + 11}px;width:${to - from}px"></i>`));
      shot.appendChild(make(`<b class="co" style="left:${x}px;top:${y}px">${el.dataset.co}</b>`));
    }
  };
  document.fonts.ready.then(place);
  window.placeMarks = place;
})();
