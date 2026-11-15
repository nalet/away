// Every text of Away's pages, in one place, in English.

export const texts = {
  name: 'away',
  skip: 'skip to the page',
  views: { home: 'home', programs: 'programs', machines: 'machines', sessions: 'sessions', administration: 'administration', settings: 'settings' },
  gate: {
    how: "You sign in with your organisation's account.",
    signIn: 'sign in',
    signedOut: "You're signed out.",
  },
  failed: {
    title: "couldn't sign you in",
    'no-answer': "The sign-in service didn't answer.",
    unusable: "The sign-in service's answer can't be used. Something is wrong with how Away is set up, not with your account.",
    again: 'try again',
    details: 'details',
  },
  contact: (whom: string) => `Questions go to ${whom}.`,
  ranOut: {
    title: 'sign in again',
    why: 'Your sign-in has run out.',
    signIn: 'sign in',
  },
  notSetUp: {
    title: 'nobody can sign in yet',
    why: 'This installation is told of no sign-in service. The installation guide says how to name one.',
  },
  unreached: {
    title: "away doesn't answer",
    why: "The page couldn't reach Away.",
    again: 'try again',
  },
  menu: {
    account: (name: string) => `${name}, your account`,
    settings: 'settings',
    signOut: 'sign out',
  },
  home: {
    unanswered: "couldn't load the machines",
    again: 'try again',
    others: 'other machines',
    machines: 'machines',
  },
  // the pages of what one may open: home, the programs, the machines
  opens: {
    home: {
      none: 'no machines for you yet',
      find: 'find a machine or a program…',
      noMatch: (sought: string) => `Nothing matches “${sought}”.`,
      looksAt: 'The search looks at the names, the addresses and the groups of the machines you can reach, and at the names of their programs.',
    },
    programs: {
      none: 'no programs for you yet',
      find: 'find a program…',
      noMatch: (sought: string) => `No program matches “${sought}”.`,
      looksAt: 'The search looks at the names of the programs you can open, and at the names, the addresses and the groups of their machines.',
    },
    machines: {
      none: 'no machines for you yet',
      find: 'find a machine…',
      noMatch: (sought: string) => `No machine matches “${sought}”.`,
      looksAt: 'The search looks at the names, the addresses and the groups of the machines you can reach.',
    },
  },
  machine: {
    back: 'machines',
    notHere: 'no machine of that name for you',
    notHereWhy: "It doesn't exist, or no rule gives it to you.",
    home: 'go home',
    notYet: "This version of Away opens Windows desktops and the programs of Windows machines. It doesn't open shells or other desktops yet.",
    desktop: 'desktop',
    programs: 'programs',
    shell: 'shell',
    about: 'about this machine',
    whatItIs: 'What it is',
    kinds: { windows: 'Windows machine', linux: 'Linux machine', mac: 'Mac' },
    collection: 'Collection',
    address: 'Address',
    groups: 'Groups',
    inNone: 'none',
    // what a way in is reached over, said for people
    desktopOf: { windows: 'Windows desktop', linux: 'Linux desktop', mac: 'Mac desktop' },
    nativeRdp: {
      title: 'Restricted Admin in Remote Desktop Connection',
      how: 'On your Windows admin workstation, press Windows+R, paste this command, then press Enter.',
      command: 'Remote Desktop command',
      copy: 'Copy command',
      copied: 'Command copied.',
      select: 'Command selected. Press Ctrl+C to copy it.',
    },
    shellOf: 'A shell on the machine',
  },
  // what a machine is, in one line under its name
  kindNames: { windows: 'Windows', linux: 'Linux', mac: 'Mac' },
  programCount: (count: number) => (count === 1 ? '1 program' : `${count} programs`),
  programOn: (machine: string) => `program on ${machine}`,
  stars: {
    favourites: 'favourites',
    add: 'add to favourites',
    remove: 'remove from favourites',
    // a star's name for whoever doesn't see it: what it does, and to what
    named: (what: string, name: string) => `${what}: ${name}`,
    none: 'nothing on home yet',
    noneWhy: 'What you star is here. Every program and every machine you can reach is on its own page.',
  },
  sessions: {
    title: 'no sessions are open',
  },
  // a session with a machine: the steps of the connection, what the machine asks, and how it ends
  session: {
    stepsOf: (machine: string) => `the steps to ${machine}`,
    steps: {
      reaching: (machine: string) => `Reaching ${machine}`,
      checking: (machine: string) => `Checking that it is ${machine}`,
      'signing-in': (machine: string) => `Signing in on ${machine}`,
      'first-picture': () => 'Waiting for its first picture',
    },
    done: 'done',
    running: 'running',
    failedAt: 'failed',
    asks: (machine: string) => `${machine} asks for a name and a password. Away passes them on and doesn't keep them.`,
    // a person Away signs in as themselves on no machine, on a machine where others are
    asksInstead: (machine: string) =>
      `Away signs you in on no machine as yourself, so ${machine} asks for a name and a password. Away passes them on and doesn't keep them.`,
    asksAgain: (machine: string) => `${machine} didn't accept that name and password.`,
    name: 'Name',
    password: 'Password',
    signIn: 'sign in',
    cancel: 'cancel',
    machines: 'machines',
    tryAgain: 'try again',
    openAgain: 'open again',
    bringHere: 'bring it here',
    details: 'details',
    // a step that failed: what it says, and whether trying again can help
    failed: (step: string, why: string, machine: string, address: string): { says: string; again: boolean } => {
      const known: Record<string, { says: string; again: boolean }> = {
        'reaching no-answer': { says: "It didn't answer within 10 seconds. It may be switched off, or its address may have changed.", again: true },
        'reaching no-address': { says: `Its name, ${address}, leads to no address.`, again: true },
        'reaching refused': { says: 'It refused the connection. Remote access may be switched off there.', again: true },
        'checking another-identity': { says: "It showed another identity than the one Away knows, so Away doesn't connect to it.", again: false },
        'signing-in sign-in-refused': { says: `${machine} didn't accept that name and password.`, again: true },
        'signing-in not-as-yourself': { says: `Away couldn't sign you in on ${machine} as yourself.`, again: true },
        'signing-in sent-elsewhere': { says: "Its farm sent Away on to a machine that isn't among those named for it, so Away doesn't connect there.", again: false },
        'first-picture no-picture': { says: 'It took the sign-in and then showed nothing in time.', again: true },
        'first-picture program-not-started': { says: "It took the sign-in and didn't start the program.", again: true },
      };
      return known[`${step} ${why}`] ?? { says: 'Something went wrong on the way to it.', again: true };
    },
    // the server didn't begin the session: what the page says, and whether trying again can help
    refused: (status: number, machine: string): { title: string; why: string; again: boolean } => {
      if (status === 404) return { title: 'no machine of that name for you', why: "It doesn't exist, or no rule gives it to you.", again: false };
      if (status === 501) return { title: `${machine} can't be opened yet`, why: 'This version of Away opens Windows desktops and programs, and nothing else yet.', again: false };
      if (status === 503) return { title: "away can't open machines right now", why: "No machine is opened without a record, and Away can't write its records right now.", again: true };
      return { title: `couldn't open ${machine}`, why: 'Away did not begin the session.', again: true };
    },
    // a session that ended: what the page says, and whether it can be opened again
    ended: (why: string, machine: string): { title: string; why: string; again: boolean; bring?: boolean } => {
      if (why === 'machine') return { title: `${machine} closed this session`, why: 'It was shut down or restarted, or you were signed out there.', again: true };
      // a desktop is taken along: opened on another device, it is there, and this one says so
      if (why === 'elsewhere') {
        return {
          title: 'this desktop is open somewhere else',
          why: `You opened ${machine} in another window or on another device. A desktop is in one place at a time.`,
          again: true,
          bring: true,
        };
      }
      return { title: `couldn't open ${machine}`, why: 'Away did not begin the session.', again: true };
    },
    screenOf: (machine: string) => `the desktop of ${machine}`,
    windowsOf: (program: string) => `the windows of ${program}`,
    windows: 'windows',
    toFront: (title: string) => `bring to the front: ${title}`,
    closeWindow: (title: string) => `close: ${title}`,
    openBar: "open the session's bar",
    bar: "the session's bar",
    keys: 'keys',
    // the key combinations a browser or a device keeps for itself
    kept: { ctrlAltDel: 'ctrl + alt + del', altTab: 'alt + tab', altF4: 'alt + f4', windows: 'windows key', ctrlW: 'ctrl + w' },
    // a desktop whose connection dropped: the card over it while Away tries again
    lost: {
      title: 'connection lost',
      tryingAgain: (tries: number) => {
        const times = ['second', 'third', 'fourth', 'fifth', 'sixth', 'seventh', 'eighth', 'ninth', 'tenth'];
        if (tries <= 1) return 'Trying again.';
        return times[tries - 2] ? `Trying again, for the ${times[tries - 2]} time.` : 'Trying again, once more.';
      },
      running: (machine: string, program: boolean) => (program ? `Your programs are still running on ${machine}.` : `Your desktop is still running on ${machine}.`),
      updating: 'Away is being updated.',
      comesBack: (program: boolean) => (program ? 'Your programs come back by themselves.' : 'Your desktop comes back by itself.'),
      leave: 'leave',
      tryNow: 'try now',
    },
    // the clipboard: the bar's button, and the panel under it
    clipboard: {
      name: 'clipboard',
      passes: 'Text you copy on one side can be pasted on the other.',
      passesTo: (machine: string) => `Text you copy here can be pasted on ${machine}.`,
      passesFrom: (machine: string) => `Text you copy on ${machine} can be pasted here.`,
      notByItself: "This browser doesn't pass its clipboard on by itself.",
      pasteHere: (machine: string) => `Paste here what ${machine} should have in its clipboard.`,
      toMachine: 'To the machine',
      send: 'send',
      fromMachine: 'From the machine',
      copy: 'copy',
      copiedHere: 'It is in your clipboard now.',
      noCopying: (machine: string) => `Copying from ${machine} is switched off.`,
      noPasting: (machine: string) => `Pasting to ${machine} is switched off.`,
    },
    fill: 'fill the screen',
    unfill: 'leave the full screen',
    end: 'end',
    closeBar: 'close the bar',
  },
  administration: {
    tabs: { machines: 'machines', rules: 'rules', farms: 'farms' },
    unanswered: "couldn't load what is in force",
    again: 'try again',
    noFiles: 'no files of machines and rules are named',
    noFilesWhy: 'This installation is told of no folder of files. The installation guide says how to name one.',
    since: (when: string) => `In force since ${when}, when the files were last applied.`,
    toChange: {
      machines: 'To change a machine, change its file.',
      rules: 'To change a rule, change its file.',
      farms: 'A farm says itself what it publishes: Away asks its feed every few minutes.',
    },
    farm: 'farm',
    feed: 'feed',
    publishes: 'publishes',
    lastRead: 'last read',
    notRead: 'not yet',
    published: (desktops: number, programs: number) =>
      `${desktops === 1 ? '1 desktop' : `${desktops} desktops`}, ${programs === 1 ? '1 program' : `${programs} programs`}`,
    noFarms: 'The files name no farms.',
    feedFailed: (farm: string) => `The feed of ${farm} could not be read.`,
    whatStays: 'What it published before stays.',
    leftOut: (farm: string, names: string) => `Left out of what ${farm} publishes, because a machine of the files has the name: ${names}.`,
    refused: (file: string, when: string) => `A change to ${file} was not applied on ${when}.`,
    refusedSet: (when: string) => `A change to the files was not applied on ${when}.`,
    inLine: (line: number, why: string) => `In line ${line}, ${why}.`,
    whole: (why: string) => `${why.charAt(0).toUpperCase()}${why.slice(1)}.`,
    stays: 'What was in force before stays.',
    nothing: 'Nothing is in force yet.',
    filter: 'filter machines…',
    machine: 'machine',
    whatItIs: 'what it is',
    groups: 'groups',
    reachedBy: 'who reaches it',
    file: 'file',
    rule: 'rule',
    who: 'who',
    reaches: 'reaches',
    noMachines: 'The files name no machines.',
    noRules: 'The files name no rules, so nobody reaches a machine.',
    noMatch: 'No machine matches.',
    nobody: 'nobody',
    givesTo: (rule: string, who: string) => `${rule} gives it to ${who}`,
  },
  nowhere: {
    title: 'nothing is at this address',
    home: 'go home',
  },
  settings: {
    account: 'account',
    about: 'about',
    aboutAway: 'About Away',
    aboutWhat: (version: string) => `Version ${version}, its source and its licences.`,
  },
  about: {
    title: 'about away',
    back: 'settings',
    thisAway: 'this away',
    version: 'Version',
    built: 'Built from',
    of: 'of',
    unnamed: 'a commit the build was not told',
    licence: 'Licence',
    licences: { 'MPL-2.0': 'Mozilla Public License 2.0' } as Record<string, string>,
    itsText: 'its text',
    nobody: 'This Away asks nobody outside for anything: no fonts, no scripts, no statistics and no checks for updates.',
    hole: 'Found a security hole?',
    report: 'How to report it in private',
    builtFrom: 'what it is built from',
    parts: (count: number) => `${count} parts`,
    partsOf: (shown: number, count: number) => `${shown} of ${count} parts`,
    find: 'find a part…',
    part: 'part',
    partVersion: 'version',
    partLicence: 'licence',
    none: 'No part has that in its name or its licence.',
    notSaid: "Away didn't say what it is built from.",
    again: 'try again',
  },
} as const;
