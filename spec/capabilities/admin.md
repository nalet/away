# Administration

What an administrator does: saying which machines there are and how each is
reached, who reaches which, and reading what happened.
[How to read the tables](./README.md#how-to-read-it).

<!-- generated: areas -->
## The register of machines

Which machines there are, how each is reached and how people are signed in on it. For the goal [Know the machines](../goals/README.md#know-the-machines).

| Capability | Who | Away | Key |
|---|---|---|---|
| Keep machines, groups and rules as files, applied as they change | operator | v1 | `keep-as-files` |
| See the machines, groups and rules that are in force, and the file each comes from | admin | v1 | `see-what-is-in-force` |
| See that a changed file was refused, and why, while what was in force stays | operator | v1 | `see-refused-change` |
| Add a machine in the browser: its address, its kind, how one gets in | admin | later | `add-machine-in-browser` |
| Put machines in groups | admin | v1 | `group-machines` |
| Say for a machine how people are signed in: as themselves, with a kept secret, or by typing | admin | v1 | `set-sign-in-way` |
| Keep a name and password or a key for a machine, never shown again | admin | v1 | `keep-secret` |
| Change a kept password on the machine by itself from time to time | system | no | `change-kept-secret` |
| Say which programs of a Windows machine people may open | admin | v1 | `offer-programs` |
| Give a program an icon of its own, which people know it by | admin | v1 | `give-program-icons` |
| Read the programs a Windows machine publishes from the machine itself | system | later | `read-programs-from-host` |
| Set how a machine is reached: its port, its screen, what may pass | admin | v1 | `set-connection-options` |
| Test a machine: is it reached, does signing in work, and if not, which step fails | admin | v1 | `test-machine` |
| Say how a machine proves who it is, and accept a changed identity on purpose | admin | v1 | `know-machine-identity` |
| Take machines over from the organisation's directory, and keep them in step | system | later | `take-from-directory` |
| Take virtual machines over from a hypervisor | system | later | `take-from-hypervisor` |
| Close a machine for a while, and say why | admin | later | `close-for-upkeep` |
| Make, copy or hand out fresh desktops as people ask for them | system | no | `make-desktops` |
| Reach a machine through a helper installed on it | system | no | `install-helper-on-machine` |

- *Keep machines, groups and rules as files, applied as they change*: so they can live in a repository and be reviewed like code.
- *Keep a name and password or a key for a machine, never shown again*: in the first release the machine's file names a secret that the cluster holds; typing one in the browser comes with adding machines there.
- *Change a kept password on the machine by itself from time to time*: Away keeps the secrets it needs and manages no others.
- *Give a program an icon of its own, which people know it by*: a picture among the files; Away serves it itself.
- *Say how a machine proves who it is, and accept a changed identity on purpose*: by an authority Away trusts, or by what the machine showed when it was first tested.
- *Make, copy or hand out fresh desktops as people ask for them*: Away reaches machines that exist.
- *Reach a machine through a helper installed on it*: a machine is reached the way it already offers.

## Who reaches what

The rules: which people reach which machines, and what they may do there. For the goal [Decide who reaches what](../goals/README.md#decide-who-reaches-what).

| Capability | Who | Away | Key |
|---|---|---|---|
| Allow a group of people to reach a machine or a group of machines | admin | v1 | `allow-by-group` |
| Show a person only the machines a rule gives them | system | v1 | `show-only-allowed` |
| See what a person reaches, and by which rule | admin | v1 | `see-who-reaches` |
| Take people and their groups from the identity provider, and keep none of its own | system | built | `take-people-from-provider` |
| Say who administers Away, who reads its records, and who only connects | admin | built | `name-roles` |
| Allow or forbid the clipboard, in each direction | admin | v1 | `limit-clipboard` |
| Allow or forbid moving files, in each direction | admin | later | `limit-files` |
| Allow or forbid printing, sound and the microphone | admin | later | `limit-other-channels` |
| Allow looking at a machine without touching it | admin | later | `allow-watch-only` |
| End a session nobody has used for a set time, with a warning before | system | v1 | `end-when-idle` |
| End a session after a longest time, with a warning before | system | later | `limit-length` |
| Limit how many sessions a person or a machine has at once | system | later | `limit-count` |
| Allow a machine at certain hours only | admin | later | `limit-by-time` |
| Allow a machine from certain networks or kinds of device only | admin | later | `limit-by-place` |
| Ask for a fresh sign-in or a second factor before a machine opens | admin | later | `ask-again-first` |
| Let a person ask for a machine, and someone allow it for a while | person | later | `ask-for-access` |
| Ask why before a machine opens, and keep the answer | admin | later | `ask-for-reason` |
| Say by rule which account a group is on a machine as: their own, or a shared one | admin | later | `set-account-by-rule` |
| Let an administrator in while the identity provider is down | operator | later | `get-in-without-provider` |

## Sessions as they run

Who is connected right now, and ending what should not be. For the goal [Decide who reaches what](../goals/README.md#decide-who-reaches-what).

| Capability | Who | Away | Key |
|---|---|---|---|
| See who is connected to what right now | admin | v1 | `see-live-sessions` |
| End a session, or all of a person's | admin | v1 | `end-session` |
| End a person's sessions as soon as their right or their account is gone | system | v1 | `end-on-lost-right` |
| Watch a running session without taking part | admin | later | `watch-session` |
| Send a message to the people in a session | admin | later | `message-session` |

- *End a person's sessions as soon as their right or their account is gone*: an account that is gone shows as a sign-in that can't be renewed: its sessions stop at once and end after the time the operator set.

## What happened

The records of every connection and change, and the recordings of sessions. For the goal [Know what happened](../goals/README.md#know-what-happened).

| Capability | Who | RDP | VNC | SSH | Away | Key |
|---|---|---|---|---|---|---|
| Keep a record of every connection: who, which machine, as whom, when, from where, how it ended | system | — | — | — | v1 | `record-connections` |
| Keep a record of every sign-in Away made on a machine for someone | system | — | — | — | v1 | `record-sign-ins-made` |
| Keep a record of every change to machines, rules and settings | system | — | — | — | v1 | `record-changes` |
| Read the records and search them | admin | — | — | — | v1 | `read-records` |
| Send every record on to the organisation's own log system as it is written | system | — | — | — | v1 | `send-records-on` |
| Keep a record of every file moved and of what the clipboard carried | system | — | — | — | later | `record-moved-content` |
| Record a session to play it back later | system | later | later | later | — | `record-session` |
| Play a recording back, at any speed and from any point | admin | later | later | later | — | `play-recording` |
| Find a moment in a recording by what was typed or shown | admin | no | no | later | — | `search-recording` |
| Tell a person that their session is recorded, before it starts | system | — | — | — | later | `tell-when-recorded` |
| Keep records and recordings for a set time, then remove them | system | — | — | — | later | `keep-for-set-time` |
| Make a changed or missing record show | system | — | — | — | later | `show-tampering` |
| See how Away is used over time: by whom, which machines, for how long | admin | — | — | — | later | `see-use-over-time` |

- *Send every record on to the organisation's own log system as it is written*: as lines a log collector reads.
- *Find a moment in a recording by what was typed or shown*: a shell is recorded as text, a desktop as pictures.
<!-- end: areas -->
