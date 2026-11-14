// Every text of Away's pages, in one place, in English.

export const texts = {
  name: 'away',
  skip: 'skip to the page',
  views: { home: 'home', sessions: 'sessions', administration: 'administration', settings: 'settings' },
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
    title: 'nothing to open yet',
    why: 'This version of Away signs you in. It knows no machines yet.',
  },
  sessions: {
    title: 'no sessions are open',
  },
  administration: {
    title: 'nothing to administer yet',
    why: 'This version of Away has nothing to administer yet.',
  },
  nowhere: {
    title: 'nothing is at this address',
    home: 'go home',
  },
  settings: {
    about: 'about',
    version: 'version',
    built: 'built from',
    source: 'source',
    you: 'signed in as',
  },
} as const;
