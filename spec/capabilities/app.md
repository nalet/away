# Around the sessions

What a person does before and beside a session: signing in, finding a machine,
moving between the sessions that are open, bringing someone else in.
[How to read the tables](./README.md#how-to-read-it).

<!-- generated: areas -->
## Signing in

Signing in to Away once, and from there on each machine. For the goal [Get in without another password](../goals/README.md#get-in-without-another-password).

| Capability | Who | RDP | VNC | SSH | Away | Key |
|---|---|---|---|---|---|---|
| Sign in through the organisation's identity provider | person | — | — | — | built | `sign-in` |
| Sign in through an identity provider that speaks SAML only | person | — | — | — | no | `sign-in-with-saml` |
| Stay signed in while the sign-in renews itself | system | — | — | — | built | `stay-signed-in` |
| Sign in again when the sign-in has run out, with the open sessions kept | person | — | — | — | built | `sign-in-again` |
| Sign out, and end the sessions Away opened | person | — | — | — | built | `sign-out` |
| Be signed in on the machine as oneself, with nothing asked | system | v1 | no | v1 | — | `be-oneself-on-machine` |
| Be signed in with a name and password or a key that Away keeps for the machine | system | v1 | v1 | v1 | — | `use-kept-secret` |
| Type a name and a password when the machine asks for them | person | v1 | v1 | v1 | — | `type-credentials` |
| Have Away remember what one typed for a machine | person | later | later | later | — | `remember-typed-credentials` |
| Choose which of one's accounts to be on the machine as | person | later | — | later | — | `choose-account` |
| Reach file shares and printers from the machine as oneself | system | v1 | — | — | — | `reach-network-as-oneself` |
| Go on from the machine to a further one with nothing asked | person | no | — | no | — | `go-on-without-password` |
| Open a locked desktop again with nothing asked | person | v1 | later | — | — | `unlock-without-password` |
| Be signed in as oneself on a Windows machine that is joined to a cloud directory only | system | later | — | — | — | `be-oneself-cloud-joined` |
| Check that a machine is the one it claims to be, and refuse one that changed | system | v1 | later | v1 | — | `check-machine-identity` |

- *Sign in through the organisation's identity provider*: over OpenID Connect.
- *Sign in through an identity provider that speaks SAML only*: OpenID Connect is the one way in.
- *Be signed in on the machine as oneself, with nothing asked*: a Windows machine in a domain, and a shell that trusts Away; an ordinary account is enough; VNC knows no identity.
- *Be signed in with a name and password or a key that Away keeps for the machine*: for a machine that knows no directory; the secret never reaches the browser.
- *Go on from the machine to a further one with nothing asked*: the further machine is opened from Away: every connection starts there.

## The frame

What is around every page: the rail, the top bar, the search, and the bar of a session. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | Away | Key |
|---|---|---|---|
| Move between the machines, the open sessions and the settings | person | built | `move-between-views` |
| See the sessions that are open, and go to one | person | v1 | `see-open-sessions` |
| Find a machine by its name from any page | person | v1 | `find-from-anywhere` |
| See which account is signed in | person | built | `see-who-is-signed-in` |
| Reach a session's own controls without leaving it: keys, clipboard, files, size, ending it | person | v1 | `use-session-bar` |
| Give the machine the whole window while a session is open | system | v1 | `give-session-the-window` |
| Use every page around a session with the keyboard alone, and with a screen reader | person | built | `use-without-mouse` |
| Use Away in one's own language | person | later | `choose-language` |
| Be told what happened meanwhile: a session ended, a request was answered | person | later | `see-notices` |
| Use an installed app instead of the browser, on a computer or a phone | person | no | `use-installed-app` |

- *Move between the machines, the open sessions and the settings*: a rail of icons on a wide screen, a bar of icons on a phone.
- *Give the machine the whole window while a session is open*: a session with programs keeps the list of their windows beside them.
- *Use Away in one's own language*: English first.
- *Use an installed app instead of the browser, on a computer or a phone*: the browser is the one way in.

## Finding a machine

The machines a person may reach: the list, its groups, the search, what was used last. For the goal [Know the machines](../goals/README.md#know-the-machines).

| Capability | Who | Away | Key |
|---|---|---|---|
| See the machines one may reach | person | v1 | `see-machines` |
| Find a machine by its name, its address or its group | person | v1 | `search-machines` |
| See the machines in the groups an administrator made | person | v1 | `see-groups` |
| See the machines used last | person | v1 | `see-last-used` |
| Mark machines as favourites and see them first | person | v1 | `keep-favourites` |
| See what a machine is: its kind, its address, how one gets in | person | v1 | `see-machine-details` |
| See the programs a Windows machine offers, and open one | person | v1 | `see-programs` |
| Choose how to open a machine that offers more than one way: its desktop, its programs, its shell | person | v1 | `choose-way-in` |
| Open a machine from a link or a bookmark | person | v1 | `open-by-address` |
| See whether a machine is on and can be reached | person | later | `see-machine-state` |
| Wake a machine that sleeps or is off, and connect once it is up | person | later | `wake-machine` |
| Add a machine of one's own that nobody else sees | person | later | `add-own-machine` |

- *Wake a machine that sleeps or is off, and connect once it is up*: needs a way into the machine's own network segment, or to its hypervisor.

## Working together

Two or more people in one session. For the goal [Look at a machine together](../goals/README.md#look-at-a-machine-together).

| Capability | Who | Away | Key |
|---|---|---|---|
| Invite someone into a running session with a link | person | later | `invite-to-session` |
| Let someone watch without touching | person | later | `let-watch-only` |
| Hand the keyboard and the mouse to someone else, and take them back | person | later | `hand-over-control` |
| See who is in the session | person | later | `see-who-is-there` |
| Join a person's session to help, once they agree | admin | later | `join-to-help` |
| Invite someone who has no account | person | no | `invite-without-account` |

- *Invite someone who has no account*: everyone in a session is known to the identity provider.

## One's own settings

What a person sets for themselves. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | Away | Key |
|---|---|---|---|
| Set how one's sessions open: their size, the picture, the keyboard layout | person | v1 | `set-own-defaults` |
| Keep one's settings for one machine apart from the rest | person | later | `set-per-machine` |
| See one's own past sessions | person | later | `see-own-sessions` |
| See and remove what Away remembers for oneself | person | later | `forget-remembered` |
| Report a problem, with what Away knows of the session attached | person | later | `report-problem` |
<!-- end: areas -->
