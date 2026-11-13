// What a machine shows, in the drawings: stand-ins for a desktop, a program's
// window and a shell, so every chapter draws them alike. Load it before frame.js.
// None of it is Away's own, and none of it is a real system's look.
//
//   <div class="remote desk" data-desk="work"></div>
//       a desktop with two windows open. data-desk="work" (a bar at the foot),
//       "top" (a bar at the top and a dock, as other desktops have it), "signin"
//       (the machine's own sign-in screen), "empty" (the desktop alone).
//   <div class="w front" data-w="sheet" style="left:…;top:…;width:…;height:…"></div>
//       one window: "sheet" (a table of figures), "doc" (a text), "mail" (a list
//       beside a text). data-title sets its title.
//   <div class="remote term" data-term="shell"></div>
//       a shell: "shell" (commands and their output), "monitor" (a program that
//       takes the whole shell), "editor" (a text editor), "phone" (a few short lines).
//   A thumbnail is the same, drawn at full size and scaled down:
//   <div class="thumb"><div class="remote desk" data-desk="work"></div></div>
(function () {
  const lines = (ws) => ws.map((w) => (w === 't' ? '<p class="t"></p>' : w === 0 ? '<p class="gap"></p>' : `<p style="width:${w}%"></p>`)).join('');
  const DOC = lines(['t', 96, 88, 93, 71, 0, 94, 90, 97, 85, 62, 0, 91, 95, 78]);
  const BUDGET = [
    ['Item', 'Q1', 'Q2', 'Q3', 'Q4', 'Year'],
    ['Licences', '12 400', '12 400', '12 900', '12 900', '50 600'],
    ['Hosting', '8 150', '8 300', '8 300', '8 720', '33 470'],
    ['Support', '4 000', '4 000', '4 000', '4 000', '16 000'],
    ['Training', '1 200', '', '2 400', '', '3 600'],
    ['Travel', '960', '1 430', '310', '1 180', '3 880'],
    ['Total', '26 710', '26 130', '27 910', '26 800', '107 550'],
  ];
  // a table of figures: letters over the columns, numbers before the rows, 88 × 22 a cell
  const sheet = (rows) => {
    let out = '';
    for (let c = 0; c < 12; c++) out += `<b style="left:${40 + c * 88}px;top:0;width:88px;text-align:center;color:#5d6b7c">${String.fromCharCode(65 + c)}</b>`;
    for (let r = 0; r < 30; r++) out += `<b style="left:0;top:${22 + r * 22}px;width:40px;text-align:center;color:#5d6b7c">${r + 1}</b>`;
    rows.forEach((row, r) => row.forEach((v, c) => {
      const cls = [r === 0 || r === rows.length - 1 ? 'h' : '', c ? 'n' : '', r === 2 && c === 3 ? 'sel' : ''].filter(Boolean).join(' ');
      out += `<b class="${cls}" style="left:${40 + c * 88}px;top:${22 + r * 22}px">${v}</b>`;
    }));
    return `<div class="sheet">${out}</div>`;
  };
  const WINDOWS = {
    sheet: { title: 'Budget 2027 – Tables', menu: ['File', 'Edit', 'Insert', 'Data', 'View'], body: sheet(BUDGET) },
    doc: { title: 'Minutes 12 November – Text', menu: ['File', 'Edit', 'Format', 'View'], body: `<div class="doc">${DOC}</div>` },
    mail: { title: 'Inbox – Mail', menu: ['New', 'Reply', 'Forward', 'Archive'],
            body: `<div class="wside">${lines([70, 86, 54]).replace('<p ', '<p class="on" ')}${lines([0, 80, 66, 90, 58, 74])}</div><div class="doc" style="left:168px">${DOC}</div>` },
  };
  const fill = (el) => {
    const w = WINDOWS[el.dataset.w];
    if (!w) { console.error(`remote.js: no window "${el.dataset.w}"`); return; }
    el.innerHTML = `<div class="wt">${el.dataset.title || w.title}</div><div class="wm">${w.menu.map((m) => `<span>${m}</span>`).join('')}</div><div class="wb">${w.body}</div>`;
  };
  const win = (kind, cls, style) => `<div class="w ${cls}" data-w="${kind}" style="${style}"></div>`;
  const ICOS = ['Reports', 'Shared', 'Bin'].map((n, i) => `<span class="ico" style="left:12px;top:${16 + i * 84}px">${n}</span>`).join('');
  const DESKS = {
    work: () => `${ICOS}${win('doc', '', 'left:118px;top:44px;width:600px;height:430px')}${win('sheet', 'front', 'left:330px;top:128px;width:820px;height:540px')}
      <div class="tbar"><i></i><i class="on"></i><i class="on"></i><i></i><i></i><span class="clock">09:14<br>13.11.2026</span></div>`,
    empty: () => `${ICOS}<div class="tbar"><i></i><i></i><i></i><i></i><i></i><span class="clock">09:14<br>13.11.2026</span></div>`,
    signin: () => `<div class="lockbox"><span class="face"></span><span>Name</span><span class="fld"></span><span>Password</span><span class="fld"></span></div>`,
    top: () => `<div class="tbar"><i></i><span>Files</span><span>Edit</span><span>View</span><span>Go</span><span class="clock">Fri 13 Nov 09:14</span></div>
      ${win('mail', 'front', 'left:150px;top:70px;width:900px;height:560px')}
      <div class="dock"><i></i><i class="on"></i><i></i><i></i><i></i><i></i></div>`,
  };
  const P = '<span class="g">alex@build-01</span>:<span class="b">~/src/billing</span>$ ';
  const TERMS = {
    shell: () => `${P}git status --short
 <span class="y">M</span> api/invoice.rs
 <span class="y">M</span> api/tests/invoice.rs
${P}cargo test invoice
   <span class="g">Compiling</span> billing v0.14.2 (/home/alex/src/billing)
    <span class="g">Finished</span> test [unoptimized + debuginfo] target(s) in 8.41s
     <span class="g">Running</span> unittests src/lib.rs

running 6 tests
test invoice::tests::adds_lines ... <span class="g">ok</span>
test invoice::tests::rounds_half_up ... <span class="g">ok</span>
test invoice::tests::rejects_negative_total ... <span class="g">ok</span>
test invoice::tests::keeps_currency ... <span class="g">ok</span>
test invoice::tests::numbers_in_sequence ... <span class="g">ok</span>
test invoice::tests::dates_in_utc ... <span class="g">ok</span>

test result: <span class="g">ok</span>. 6 passed; 0 failed; 0 ignored; 0 measured; finished in 0.03s

${P}tail -n 3 /var/log/billing/worker.log
2026-11-13T09:13:41Z <span class="c">info</span>  batch 4411 done: 212 invoices, 0 failed
2026-11-13T09:13:58Z <span class="y">warn</span>  queue lag 41 s, above 30 s
2026-11-13T09:14:02Z <span class="c">info</span>  batch 4412 started
${P}<span class="cur"></span>`,
    monitor: () => `  <span class="c">1</span>[<span class="g">||||||||||||||||</span><span class="r">||||</span>              54.2%]   <span class="c">Tasks:</span> <span class="w1">142</span>, <span class="w1">318</span> thr; <span class="g">2</span> running
  <span class="c">2</span>[<span class="g">||||||||</span>                          21.9%]   <span class="c">Load average:</span> <span class="w1">1.42</span> 1.18 0.97
  <span class="c">3</span>[<span class="g">||||||||||||</span><span class="r">||</span>                    38.0%]   <span class="c">Uptime:</span> 41 days, 02:17:53
  <span class="c">4</span>[<span class="g">|||||</span>                             13.4%]
  <span class="c">Mem</span>[<span class="g">||||||||||||||</span><span class="b">||||</span><span class="y">||||||</span>      9.81G/15.6G]
  <span class="c">Swp</span>[                                 0K/2.00G]

<span class="barl">    PID USER       PRI  NI  VIRT   RES   SHR S  CPU% MEM%   TIME+  Command                                                   </span>
<span class="inv">  21804 alex        20   0 2318M  612M 48212 R  52.8  3.8  1:04.11 cargo test invoice                                        </span>
   1193 postgres    20   0  412M  188M  141M S   9.6  1.2 38:12.70 postgres: billing worker
  20977 alex        20   0  1.1G  301M 55120 S   6.1  1.9  0:41.06 rust-analyzer
    912 root        20   0  1.8G  118M 61004 S   2.0  0.7 12:30.44 /usr/bin/containerd
  21790 alex        20   0 19888  6012  3908 S   0.7  0.0  0:00.31 -bash
   1071 root        20   0  298M 20144 15020 S   0.3  0.1  4:11.98 /usr/sbin/sshd -D
      1 root        20   0  167M 12880  8420 S   0.0  0.1  1:20.02 /sbin/init
    684 root        19  -1 66312 18100 16944 S   0.0  0.1  0:58.37 /lib/systemd/systemd-journald
    701 systemd-t   20   0 90260  6904  6012 S   0.0  0.0  0:02.14 /lib/systemd/systemd-timesyncd
    880 root        20   0  238M  9480  7716 S   0.0  0.1  0:09.72 /usr/libexec/accounts-daemon
    903 message+    20   0  8720  5104  4208 S   0.0  0.0  0:11.40 /usr/bin/dbus-daemon --system

<span class="inv">F1</span>Help  <span class="inv">F2</span>Setup <span class="inv">F3</span>Search<span class="inv">F4</span>Filter<span class="inv">F5</span>Tree  <span class="inv">F6</span>SortBy<span class="inv">F9</span>Kill  <span class="inv">F10</span>Quit`,
    editor: () => `<span class="d">  1 </span><span class="p">use</span> crate::money::Money;
<span class="d">  2 </span>
<span class="d">  3 </span><span class="d">/// The total of an invoice, rounded half up to the currency's unit.</span>
<span class="d">  4 </span><span class="p">pub fn</span> <span class="b">total</span>(lines: &amp;[Line]) -&gt; <span class="y">Result</span>&lt;Money, Error&gt; {
<span class="d">  5 </span>    <span class="p">let</span> sum = lines.iter().map(Line::amount).sum::&lt;Money&gt;();
<span class="d">  6 </span>    <span class="p">if</span> sum.is_negative() {
<span class="d">  7 </span>        <span class="p">return</span> <span class="y">Err</span>(Error::NegativeTotal(sum));
<span class="d">  8 </span>    }
<span class="d">  9 </span>    <span class="y">Ok</span>(sum.round_half_up())<span class="cur"></span>
<span class="d"> 10 </span>}
<span class="d">~</span>
<span class="d">~</span>
<span class="d">~</span>
<span class="inv"> api/invoice.rs [+]                                                   9,29   All </span>
<span class="w1">-- INSERT --</span>`,
    phone: () => `<span class="g">alex@build-01</span>:<span class="b">~</span>$ uptime
 09:14:02 up 41 days,  2:17
 load average: 1.42, 1.18, 0.97
<span class="g">alex@build-01</span>:<span class="b">~</span>$ systemctl is-active billing
<span class="g">active</span>
<span class="g">alex@build-01</span>:<span class="b">~</span>$ df -h /var
Filesystem  Size  Used Avail Use%
/dev/sda2   197G  121G   67G  65%
<span class="g">alex@build-01</span>:<span class="b">~</span>$ <span class="cur"></span>`,
  };
  for (const el of document.querySelectorAll('.remote.desk[data-desk]')) {
    const d = DESKS[el.dataset.desk];
    if (!d) { console.error(`remote.js: no desktop "${el.dataset.desk}"`); continue; }
    if (el.dataset.desk === 'top') el.classList.add('topbar');
    el.insertAdjacentHTML('afterbegin', d());
  }
  for (const el of document.querySelectorAll('.remote.term[data-term]')) {
    const t = TERMS[el.dataset.term];
    if (!t) { console.error(`remote.js: no shell "${el.dataset.term}"`); continue; }
    el.insertAdjacentHTML('afterbegin', t());
  }
  for (const el of document.querySelectorAll('.w[data-w]')) fill(el);
})();
