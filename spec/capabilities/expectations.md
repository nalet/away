# Expectations

What people expect of remote access they run themselves, and what the plan makes
of each. The [capability model](./README.md) says what Away does; it cannot say
what someone looks for and doesn't find. This list does: each row is one
expectation, with the capabilities that meet it, under the
[goal](../goals/README.md) it is for.

> **Written by hand, counted by a program.** The expectations are
> [expectations.csv](./expectations.csv); the state of each is computed from the
> capabilities it names, never typed. An expectation is no requirement: it says
> what people look for, not what Away will build.

## How to read it

**State** is what the model says of an expectation as a whole:

| State | Means |
|---|---|
| there | every capability it names is built |
| partly | some of what it names is built |
| first release | nothing of it is built yet, and everything it names is planned for the first release |
| first release in part | some of what it names is planned for the first release, the rest later or not at all |
| later | nothing it names is planned for the first release |
| not planned | everything it names is left out |

**Met by** names each capability with where it stands as a whole. A
capability's key leads to its area's table; the
[map](https://nalet.github.io/away/spec/capabilities/map/) shows the same
capabilities as tiles.

A row that is later, or not planned, is a question for the owner: move it into
the first release, leave it for later, or leave it out for good.

<!-- generated: expectations -->
## Work on a machine that is somewhere else

For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Expected | State | Met by | Note |
|---|---|---|---|
| A Windows desktop in the browser | partly | [`open-desktop`](./session.md#opening-a-machine) partly built, [`fit-to-window`](./session.md#the-picture) partly built, [`desktop-as-at-machine`](./session.md#the-picture) first release | — |
| One program of a Windows machine, without its desktop | there | [`open-program`](./session.md#opening-a-machine) built, [`see-programs`](./app.md#finding-a-machine) built, [`see-all-programs`](./app.md#finding-a-machine) built, [`offer-programs`](./admin.md#the-register-of-machines) built | — |
| Several programs side by side, with a way to switch between them | partly | [`switch-programs`](./session.md#opening-a-machine) first release, [`list-open-windows`](./session.md#opening-a-machine) partly built | — |
| A Mac or a Linux desktop in the browser | partly | [`open-desktop`](./session.md#opening-a-machine) partly built, [`fit-to-window`](./session.md#the-picture) partly built, [`pointer-without-delay`](./session.md#the-picture) first release | — |
| A shell in the browser | first release | [`open-shell`](./session.md#opening-a-machine) first release, [`run-full-screen-programs`](./session.md#the-shell) first release, [`scroll-back`](./session.md#the-shell) first release, [`select-shell-text`](./session.md#the-shell) first release | — |
| Copy and paste between the machine and one's own device | partly | [`copy-paste-text`](./session.md#clipboard-files-and-printing) partly built, [`copy-paste-rich`](./session.md#clipboard-files-and-printing) partly built, [`copy-paste-files`](./session.md#clipboard-files-and-printing) partly built | — |
| Move files to and from the machine | partly | [`move-files`](./session.md#clipboard-files-and-printing) later, [`drop-file`](./session.md#clipboard-files-and-printing) partly built | — |
| Hear the machine and speak to it | later | [`hear-sound`](./session.md#sound-and-devices) later, [`use-microphone`](./session.md#sound-and-devices) later | — |
| Print from the machine | later | [`print-to-own-device`](./session.md#clipboard-files-and-printing) later | — |
| The whole screen, and more than one | partly | [`fill-screen`](./session.md#the-picture) partly built, [`several-screens`](./session.md#the-picture) later | — |
| A sharp picture that stays smooth, on a slow line too | partly | [`exact-at-rest`](./session.md#the-picture) partly built, [`adapt-to-line`](./session.md#the-picture) first release, [`sharp-on-dense-screen`](./session.md#the-picture) first release, [`show-whole-pictures`](./session.md#the-picture) later, [`machine-made-pictures`](./session.md#the-picture) later, [`play-video-smoothly`](./session.md#the-picture) later | — |
| Every key and shortcut reaches the machine | partly | [`type-in-own-layout`](./session.md#keyboard-and-pointer) partly built, [`send-kept-keys`](./session.md#keyboard-and-pointer) partly built, [`give-all-keys`](./session.md#keyboard-and-pointer) partly built, [`map-modifier-keys`](./session.md#keyboard-and-pointer) first release | — |
| Scrolling feels right, on a Mac too | partly | [`scroll-as-used-to`](./session.md#keyboard-and-pointer) partly built, [`use-mouse-fully`](./session.md#keyboard-and-pointer) partly built | — |
| Usable on a phone or a tablet | partly | [`use-on-phone`](./session.md#on-a-phone-or-a-tablet) built, [`work-by-touch`](./session.md#on-a-phone-or-a-tablet) partly built, [`zoom-by-touch`](./session.md#on-a-phone-or-a-tablet) partly built, [`use-extra-keys`](./session.md#on-a-phone-or-a-tablet) partly built | — |
| A dropped connection comes back by itself | first release | [`reconnect-by-itself`](./session.md#opening-a-machine) first release, [`keep-while-away`](./session.md#opening-a-machine) first release | — |
| Several machines open at once | first release | [`open-several`](./session.md#opening-a-machine) first release, [`open-in-own-tab`](./session.md#opening-a-machine) first release, [`see-open-sessions`](./app.md#the-frame) first release | — |
| A link or a bookmark that opens a machine | there | [`open-by-address`](./app.md#finding-a-machine) built | — |
| A connection that fails says which step failed | partly | [`see-connecting-steps`](./session.md#opening-a-machine) partly built, [`test-machine`](./admin.md#the-register-of-machines) first release | — |
| A session that ends says why, and opens again from there | first release | [`see-why-ended`](./session.md#opening-a-machine) first release | — |
| The interface in one's own language | later | [`choose-language`](./app.md#the-frame) later | — |
| Usable with the keyboard alone and with a screen reader | there | [`use-without-mouse`](./app.md#the-frame) built | — |
| An app to install instead of the browser | later | [`use-installed-app`](./app.md#the-frame) not planned, [`add-to-home-screen`](./session.md#on-a-phone-or-a-tablet) later | — |
| One's own camera, USB devices and smart cards on the machine | not planned | [`use-camera`](./session.md#sound-and-devices) not planned, [`use-own-devices`](./session.md#sound-and-devices) not planned | — |
| Other kinds of target: a serial console, a database, a web page | not planned | [`open-other-kinds`](./session.md#opening-a-machine) not planned | — |

## Get in without another password

For the goal [Get in without another password](../goals/README.md#get-in-without-another-password).

| Expected | State | Met by | Note |
|---|---|---|---|
| Sign in once, with the organisation's own sign-in | there | [`sign-in`](./app.md#signing-in) built, [`stay-signed-in`](./app.md#signing-in) built | — |
| A sign-in that runs out ends no work | there | [`sign-in-again`](./app.md#signing-in) built | — |
| No second password on a Windows machine in a domain | first release | [`be-oneself-on-machine`](./app.md#signing-in) first release, [`connect-domain-authority`](./platform.md#installing-and-configuring) first release | — |
| No second password and no key of one's own on a shell | first release | [`be-oneself-on-machine`](./app.md#signing-in) first release, [`make-shells-trust`](./platform.md#installing-and-configuring) first release | — |
| From any device, also one the organisation does not manage | partly | [`sign-in`](./app.md#signing-in) built, [`be-oneself-on-machine`](./app.md#signing-in) first release | — |
| For every person, not for administrators only | first release | [`be-oneself-on-machine`](./app.md#signing-in) first release | An ordinary account is enough |
| A machine that knows no directory opens too, with a secret nobody sees | partly | [`use-kept-secret`](./app.md#signing-in) partly built, [`keep-secret`](./admin.md#the-register-of-machines) partly built | — |
| Typing a name and a password works where nothing else does | partly | [`type-credentials`](./app.md#signing-in) partly built, [`remember-typed-credentials`](./app.md#signing-in) later | — |
| Signed in onwards too: on a file share, on a further machine | first release in part | [`reach-network-as-oneself`](./app.md#signing-in) first release, [`go-on-without-password`](./app.md#signing-in) not planned | — |
| A locked desktop opens again without the password | first release | [`unlock-without-password`](./app.md#signing-in) first release | — |
| A second factor where the organisation asks for one | partly | [`sign-in`](./app.md#signing-in) built, [`ask-again-first`](./admin.md#who-reaches-what) later | The identity provider asks for it at sign-in |
| The machine is the one it claims to be | partly | [`check-machine-identity`](./app.md#signing-in) partly built, [`know-machine-identity`](./admin.md#the-register-of-machines) partly built | — |
| Signing in through SAML | not planned | [`sign-in-with-saml`](./app.md#signing-in) not planned | — |
| Signing out ends what was opened | there | [`sign-out`](./app.md#signing-in) built | — |
| Choosing between one's ordinary and one's administrator account | later | [`choose-account`](./app.md#signing-in) later | — |
| No second password on a Windows machine joined to a cloud directory | later | [`be-oneself-cloud-joined`](./app.md#signing-in) later | — |

## Look at a machine together

For the goal [Look at a machine together](../goals/README.md#look-at-a-machine-together).

| Expected | State | Met by | Note |
|---|---|---|---|
| Invite someone into a running session with a link | later | [`invite-to-session`](./app.md#working-together) later, [`see-who-is-there`](./app.md#working-together) later | — |
| Let them watch only, or take the keyboard and the mouse | later | [`let-watch-only`](./app.md#working-together) later, [`hand-over-control`](./app.md#working-together) later | — |
| Join someone's session to help them, with their consent | later | [`join-to-help`](./app.md#working-together) later | — |
| Invite someone who has no account | not planned | [`invite-without-account`](./app.md#working-together) not planned | — |

## Know the machines

For the goal [Know the machines](../goals/README.md#know-the-machines).

| Expected | State | Met by | Note |
|---|---|---|---|
| A list of the machines one may reach, with a search | partly | [`see-machines`](./app.md#finding-a-machine) built, [`search-machines`](./app.md#finding-a-machine) built, [`find-from-anywhere`](./app.md#the-frame) first release | — |
| Machines in groups | there | [`see-groups`](./app.md#finding-a-machine) built, [`group-machines`](./admin.md#the-register-of-machines) built | — |
| One machine with several ways in: its desktop, its programs, its shell | first release | [`choose-way-in`](./app.md#finding-a-machine) first release | — |
| The machines used last, and favourites, first | partly | [`see-last-used`](./app.md#finding-a-machine) first release, [`keep-favourites`](./app.md#finding-a-machine) built | — |
| Machines come from the directory or the hypervisor by themselves | later | [`take-from-directory`](./admin.md#the-register-of-machines) later, [`take-from-hypervisor`](./admin.md#the-register-of-machines) later | — |
| See whether a machine is on | later | [`see-machine-state`](./app.md#finding-a-machine) later | — |
| Wake a sleeping machine before connecting | later | [`wake-machine`](./app.md#finding-a-machine) later | — |
| Machines and rules kept as files, in a repository | there | [`keep-as-files`](./admin.md#the-register-of-machines) built, [`see-what-is-in-force`](./admin.md#the-register-of-machines) built, [`see-refused-change`](./admin.md#the-register-of-machines) built | — |
| Add and change machines in the browser | later | [`add-machine-in-browser`](./admin.md#the-register-of-machines) later | — |
| The programs a Windows machine publishes show up by themselves | there | [`read-programs-from-host`](./admin.md#the-register-of-machines) built | — |
| A person adds a machine of their own | later | [`add-own-machine`](./app.md#finding-a-machine) later | — |
| A fresh desktop made for whoever asks | not planned | [`make-desktops`](./admin.md#the-register-of-machines) not planned | — |
| A machine in a network Away is not in is reached too | later | [`reach-other-networks`](./platform.md#installing-and-configuring) later, [`install-helper-on-machine`](./admin.md#the-register-of-machines) not planned | — |

## Decide who reaches what

For the goal [Decide who reaches what](../goals/README.md#decide-who-reaches-what).

| Expected | State | Met by | Note |
|---|---|---|---|
| Who reaches what follows the organisation's groups | there | [`allow-by-group`](./admin.md#who-reaches-what) built, [`take-people-from-provider`](./admin.md#who-reaches-what) built | — |
| A person sees only the machines they may reach | there | [`show-only-allowed`](./admin.md#who-reaches-what) built | — |
| An administrator sees what a person reaches, and why | first release | [`see-who-reaches`](./admin.md#who-reaches-what) first release | — |
| Two people do not end up on one desktop unnoticed | first release | [`see-in-use`](./session.md#opening-a-machine) first release | — |
| Rules on what may pass: the clipboard, files, printing, sound | first release in part | [`limit-clipboard`](./admin.md#who-reaches-what) first release, [`limit-files`](./admin.md#who-reaches-what) later, [`limit-other-channels`](./admin.md#who-reaches-what) later | — |
| A session ends when nobody uses it, and after a longest time | first release in part | [`end-when-idle`](./admin.md#who-reaches-what) first release, [`limit-length`](./admin.md#who-reaches-what) later | — |
| An administrator sees who is connected and ends a session | first release | [`see-live-sessions`](./admin.md#sessions-as-they-run) first release, [`end-session`](./admin.md#sessions-as-they-run) first release | — |
| Someone who leaves the organisation is out at once | first release | [`end-on-lost-right`](./admin.md#sessions-as-they-run) first release | — |
| Ask for a machine, and someone allows it for a while | later | [`ask-for-access`](./admin.md#who-reaches-what) later, [`ask-for-reason`](./admin.md#who-reaches-what) later | — |
| Rules on when and from where: hours, networks, devices | later | [`limit-by-time`](./admin.md#who-reaches-what) later, [`limit-by-place`](./admin.md#who-reaches-what) later | — |
| Roles: who administers, who reads the records, who only connects | there | [`name-roles`](./admin.md#who-reaches-what) built | — |
| Looking without touching | later | [`allow-watch-only`](./admin.md#who-reaches-what) later | — |
| A way in for an administrator while the identity provider is down | later | [`get-in-without-provider`](./admin.md#who-reaches-what) later | — |

## Know what happened

For the goal [Know what happened](../goals/README.md#know-what-happened).

| Expected | State | Met by | Note |
|---|---|---|---|
| A record of who connected to what, when, from where and as whom | partly | [`record-connections`](./admin.md#what-happened) partly built, [`record-sign-ins-made`](./admin.md#what-happened) first release, [`read-records`](./admin.md#what-happened) first release | — |
| A record of every change an administrator made | there | [`record-changes`](./admin.md#what-happened) built | — |
| Sessions recorded and played back | later | [`record-session`](./admin.md#what-happened) later, [`play-recording`](./admin.md#what-happened) later, [`tell-when-recorded`](./admin.md#what-happened) later | — |
| Find a moment in a recording by what was typed or shown | later | [`search-recording`](./admin.md#what-happened) later | — |
| Records sent on to the organisation's own log system | first release | [`send-records-on`](./admin.md#what-happened) first release | — |
| Watch a running session | later | [`watch-session`](./admin.md#sessions-as-they-run) later | — |
| Records kept for a set time, then removed | later | [`keep-for-set-time`](./admin.md#what-happened) later | — |
| A record of the files moved and of what was copied across | partly | [`record-moved-content`](./admin.md#what-happened) partly built | — |
| Records nobody can change unnoticed | first release in part | [`show-tampering`](./admin.md#what-happened) later, [`send-records-on`](./admin.md#what-happened) first release | — |

## Run it oneself

For the goal [Run it oneself](../goals/README.md#run-it-oneself).

| Expected | State | Met by | Note |
|---|---|---|---|
| It runs where there is no Windows domain | partly | [`use-own-provider`](./platform.md#installing-and-configuring) built, [`make-shells-trust`](./platform.md#installing-and-configuring) first release, [`use-kept-secret`](./app.md#signing-in) partly built, [`type-credentials`](./app.md#signing-in) partly built | people come from the sign-in service; a Windows domain is a part one turns on |
| Install it in one step | partly | [`install-with-chart`](./platform.md#installing-and-configuring) built, [`install-as-one-container`](./platform.md#installing-and-configuring) later | — |
| Runs on Kubernetes and OpenShift, without special rights | there | [`install-with-chart`](./platform.md#installing-and-configuring) built, [`run-without-special-rights`](./platform.md#installing-and-configuring) built | — |
| Works with the identity provider that is there | partly | [`use-own-provider`](./platform.md#installing-and-configuring) built, [`name-account-claim`](./platform.md#installing-and-configuring) first release | — |
| An identity provider comes along for those who have none | not planned | [`bring-provider-along`](./platform.md#installing-and-configuring) not planned | — |
| Works behind the reverse proxy and with the certificates that are there | there | [`set-address`](./platform.md#installing-and-configuring) built, [`use-own-certificates`](./platform.md#installing-and-configuring) built | — |
| A check that says what is wrong with the setup | first release | [`check-setup`](./platform.md#installing-and-configuring) first release, [`test-machine`](./admin.md#the-register-of-machines) first release | — |
| People learn whom to ask when something fails | there | [`name-support-contact`](./platform.md#installing-and-configuring) built | — |
| An update in one step, and a way back | first release in part | [`update-in-one-step`](./platform.md#running-it) first release, [`go-back-a-version`](./platform.md#running-it) later | — |
| Health and measures for the monitoring that is there | partly | [`see-health`](./platform.md#running-it) first release, [`give-measures`](./platform.md#running-it) partly built, [`write-logs`](./platform.md#running-it) built | — |
| More than one instance, for load and for failure | later | [`run-several`](./platform.md#running-it) later | — |
| Back up what it holds and bring it back | later | [`back-up`](./platform.md#running-it) later | — |
| Everything an administrator does can be done by a script | partly | [`do-by-script`](./platform.md#running-it) later, [`keep-as-files`](./admin.md#the-register-of-machines) built | — |
| A private network to the machines, for other traffic too | not planned | [`carry-other-traffic`](./platform.md#installing-and-configuring) not planned, [`forward-port`](./session.md#the-shell) not planned | — |
| What can sign people in is kept apart from the rest | there | [`sign-apart`](./platform.md#installing-and-configuring) built, [`protect-secrets`](./platform.md#installing-and-configuring) built | — |
| Kept passwords are changed on the machines from time to time | not planned | [`change-kept-secret`](./admin.md#the-register-of-machines) not planned | — |

## Trust it

For the goal [Trust it](../goals/README.md#trust-it).

| Expected | State | Met by | Note |
|---|---|---|---|
| Check that what runs is what the project built | there | [`verify-image`](./platform.md#checking-it) built, [`see-image-contents`](./platform.md#checking-it) built, [`see-version-and-licences`](./platform.md#checking-it) built | — |
| Build it oneself and run its tests | partly | [`build-from-source`](./platform.md#checking-it) built, [`run-its-tests`](./platform.md#checking-it) first release | — |
| It tells nobody what is done with it | there | [`call-nobody-else`](./platform.md#checking-it) built | — |
| A security hole can be reported in private | there | [`report-in-private`](./platform.md#checking-it) built | — |
<!-- end: expectations -->
