# Where each capability is carried

Every capability of the first release, with the components that carry it and
the work package that builds it. A capability with no component would be a
promise nobody keeps, and one with no package would never be built: the check
refuses both.

The components are named by their keys, as on [components](./components.md).
A package's number leads to it among [the work packages](../plan/README.md).
Where a capability is built for one kind of connection at a time, each package
says which.

<!-- generated: carried -->
## In a session

### Opening a machine

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Open a machine's desktop | `web` `session-view` `gateway` `rdp` `vnc` `picture` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `open-desktop` |
| Open one program of a Windows machine, without its desktop | `web` `session-view` `gateway` `rdp` | [7](../plan/README.md#7-programs) | `open-program` |
| Switch between the programs open on a machine, each in a window of its own | `session-view` `gateway` `rdp` | [7](../plan/README.md#7-programs) | `switch-programs` |
| See a machine's open windows in a list beside them, and bring one to the front, also one made small | `session-view` `gateway` `rdp` | [7](../plan/README.md#7-programs) | `list-open-windows` |
| Open a shell on a machine | `web` `session-view` `gateway` `ssh` `shell-screen` | [8](../plan/README.md#8-a-shell) | `open-shell` |
| Keep several machines open at once and move between them | `web` `session-view` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [9](../plan/README.md#9-sessions-as-they-run) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `open-several` |
| Open a session in a browser tab or window of its own | `web` `session-view` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [9](../plan/README.md#9-sessions-as-they-run) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `open-in-own-tab` |
| See how far a connection has come, and which step failed and why | `session-view` `gateway` `wire` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `see-connecting-steps` |
| Come back to a session by itself after the connection dropped | `session-view` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [9](../plan/README.md#9-sessions-as-they-run) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `reconnect-by-itself` |
| Leave a session running while the browser is closed, and pick it up again | `web` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `keep-while-away` |
| See why a session ended, and open it again from there | `web` `session-view` `server` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `see-why-ended` |
| See that a desktop is in use by someone else, where opening it would take it from them or show their screen | `web` `server` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `see-in-use` |
| Reach a farm of Windows machines through its broker and land on the right one | `gateway` `rdp` | [6](../plan/README.md#6-a-desktop-over-rdp) | `follow-broker` |
| Take the licence that a machine for many people hands every client | `gateway` `rdp` | [6](../plan/README.md#6-a-desktop-over-rdp) | `take-licence` |

- *See a machine's open windows in a list beside them, and bring one to the front, also one made small*: The machine reports each window; the list is drawn from that.
- *Leave a session running while the browser is closed, and pick it up again*: The desktop goes on at its machine; opening the machine again picks it up.
- *See that a desktop is in use by someone else, where opening it would take it from them or show their screen*: The server knows which sessions run on a machine.

### The picture

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Fit the machine's screen to the window, and follow when its size changes | `session-view` `gateway` `picture` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `fit-to-window` |
| Fill the whole screen with a session | `session-view` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `fill-screen` |
| Show a sharp picture on a high-density screen | `session-view` `gateway` `rdp` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH) | `sharp-on-dense-screen` |
| Show a screen at rest exactly, pixel for pixel | `session-view` `gateway` `picture` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `exact-at-rest` |
| Give up detail while the picture moves or the line is slow, and restore it after | `session-view` `gateway` `picture` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `adapt-to-line` |
| Show the desktop as it looks at the machine: its background, smooth fonts, its effects | `gateway` `rdp` | [6](../plan/README.md#6-a-desktop-over-rdp) | `desktop-as-at-machine` |
| Draw the pointer on one's own device, so that it moves without delay | `session-view` `gateway` `picture` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `pointer-without-delay` |

### Keyboard and pointer

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Type with one's own keyboard layout, whatever the machine's is | `session-view` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `type-in-own-layout` |
| Send the key combinations a browser or a device keeps for itself, from a menu | `session-view` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `send-kept-keys` |
| Give every key to the machine while a session fills the screen | `session-view` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `give-all-keys` |
| Use a Mac keyboard on a Windows machine and the reverse, with the keys where the hands expect them | `session-view` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `map-modifier-keys` |
| Use every mouse button and the wheel, in both directions | `session-view` `gateway` `shell-screen` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `use-mouse-fully` |
| Scroll with a trackpad as smoothly, as fast and in the direction one is used to | `session-view` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `scroll-as-used-to` |
| Type the clipboard's text into the machine key by key | `session-view` `gateway` | [11](../plan/README.md#11-a-desktop-over-vnc) | `type-clipboard` |
| Type in a language whose characters are composed of several keys | `session-view` `shell-screen` | [8](../plan/README.md#8-a-shell) | `compose-characters` |

### Clipboard, files and printing

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Copy text on one side and paste it on the other | `session-view` `gateway` `rdp` `vnc` `shell-screen` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `copy-paste-text` |
| Copy and paste a picture, or text with its formatting | `session-view` `gateway` `rdp` `wire` | [6](../plan/README.md#6-a-desktop-over-rdp) | `copy-paste-rich` |
| Copy files on one side and paste them on the other | `session-view` `gateway` `rdp` `wire` | [6](../plan/README.md#6-a-desktop-over-rdp) | `copy-paste-files` |

### The shell

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Run the programs that take the whole shell: editors, monitors, with colours and the mouse | `session-view` `shell-screen` | [8](../plan/README.md#8-a-shell) | `run-full-screen-programs` |
| Scroll back through what the shell showed, and search it | `session-view` `shell-screen` | [8](../plan/README.md#8-a-shell) | `scroll-back` |
| Select the shell's text with the mouse or the keys, and copy it | `session-view` `shell-screen` | [8](../plan/README.md#8-a-shell) | `select-shell-text` |
| Set the shell's text size, its font and its colours | `web` `session-view` | [8](../plan/README.md#8-a-shell) | `set-shell-look` |

### On a phone or a tablet

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Find a machine and open it on a phone, with every page fitted to it | `web` | [4](../plan/README.md#4-getting-in-and-the-frame) | `use-on-phone` |
| Work by touch: tap to click, hold for the other button, two fingers to scroll | `session-view` | [14](../plan/README.md#14-by-touch) | `work-by-touch` |
| Zoom into the picture with two fingers and move around it | `session-view` | [14](../plan/README.md#14-by-touch) | `zoom-by-touch` |
| Use the keys a phone's keyboard lacks: Ctrl, Alt, Esc, Tab, the arrows, the function keys | `session-view` | [14](../plan/README.md#14-by-touch) | `use-extra-keys` |
| Move the pointer as on a trackpad, for small targets | `session-view` | [14](../plan/README.md#14-by-touch) | `touch-as-trackpad` |

## Around the sessions

### Signing in

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Sign in through the organisation's identity provider | `web` `server` | [4](../plan/README.md#4-getting-in-and-the-frame) | `sign-in` |
| Stay signed in while the sign-in renews itself | `server` | [4](../plan/README.md#4-getting-in-and-the-frame) | `stay-signed-in` |
| Sign in again when the sign-in has run out, with the open sessions kept | `web` `server` `gateway` | [4](../plan/README.md#4-getting-in-and-the-frame) | `sign-in-again` |
| Sign out, and end the sessions Away opened | `web` `server` `gateway` | [4](../plan/README.md#4-getting-in-and-the-frame) | `sign-out` |
| Be signed in on the machine as oneself, with nothing asked | `gateway` `issuer` `windows-sign-in` `ssh` | [10](../plan/README.md#10-a-shell-as-oneself) (SSH), [12](../plan/README.md#12-windows-as-oneself) (RDP) | `be-oneself-on-machine` |
| Be signed in with a name and password or a key that Away keeps for the machine | `gateway` `issuer` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `use-kept-secret` |
| Type a name and a password when the machine asks for them | `session-view` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH), [11](../plan/README.md#11-a-desktop-over-vnc) (VNC) | `type-credentials` |
| Reach file shares and printers from the machine as oneself | `gateway` `windows-sign-in` | [12](../plan/README.md#12-windows-as-oneself) | `reach-network-as-oneself` |
| Open a locked desktop again with nothing asked | `gateway` `issuer` `windows-sign-in` | [12](../plan/README.md#12-windows-as-oneself) | `unlock-without-password` |
| Check that a machine is the one it claims to be, and refuse one that changed | `server` `gateway` `rdp` `ssh` | [6](../plan/README.md#6-a-desktop-over-rdp) (RDP), [8](../plan/README.md#8-a-shell) (SSH) | `check-machine-identity` |

### The frame

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Move between home, the programs, the machines, the open sessions and the settings | `web` | [4](../plan/README.md#4-getting-in-and-the-frame) | `move-between-views` |
| See the sessions that are open, and go to one | `web` | [9](../plan/README.md#9-sessions-as-they-run) | `see-open-sessions` |
| Find a machine by its name from any page | `web` `server` | [9](../plan/README.md#9-sessions-as-they-run) | `find-from-anywhere` |
| See which account is signed in | `web` `server` | [4](../plan/README.md#4-getting-in-and-the-frame) | `see-who-is-signed-in` |
| Reach a session's own controls without leaving it: keys, clipboard, files, size, ending it | `session-view` | [6](../plan/README.md#6-a-desktop-over-rdp) | `use-session-bar` |
| Give the machine the whole window while a session is open | `session-view` | [6](../plan/README.md#6-a-desktop-over-rdp) | `give-session-the-window` |
| Use every page around a session with the keyboard alone, and with a screen reader | `web` | [4](../plan/README.md#4-getting-in-and-the-frame) | `use-without-mouse` |

### Finding a machine

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| See the machines one may reach, on a page of their own | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `see-machines` |
| Find a machine by its name, its address or its group | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `search-machines` |
| See the machines in the groups an administrator made | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `see-groups` |
| See the machines used last | `web` `server` | [9](../plan/README.md#9-sessions-as-they-run) | `see-last-used` |
| Mark machines and programs as favourites, and see them on home | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `keep-favourites` |
| See what a machine is: its kind, its address, how one gets in | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `see-machine-details` |
| See the programs a Windows machine offers, and open one | `web` `server` | [7](../plan/README.md#7-programs) | `see-programs` |
| See every program one may open on a page of their own, each as an entry, and find it by its name | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `see-all-programs` |
| Choose how to open a machine that offers more than one way: its desktop, its programs, its shell | `web` | [6](../plan/README.md#6-a-desktop-over-rdp) | `choose-way-in` |
| Open a machine from a link or a bookmark | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `open-by-address` |

### One's own settings

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Set how one's sessions open: their size, the picture, the keyboard layout | `web` `server` | [6](../plan/README.md#6-a-desktop-over-rdp) | `set-own-defaults` |

## Administration

### The register of machines

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Keep machines, groups and rules as files, applied as they change | `server` `chart` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `keep-as-files` |
| See the machines, groups and rules that are in force, and the file each comes from | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `see-what-is-in-force` |
| See that a changed file was refused, and why, while what was in force stays | `web` `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `see-refused-change` |
| Put machines in groups | `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `group-machines` |
| Say for a machine how people are signed in: as themselves, with a kept secret, or by typing | `server` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) | `set-sign-in-way` |
| Keep a name and password or a key for a machine, never shown again | `issuer` `chart` | [6](../plan/README.md#6-a-desktop-over-rdp) | `keep-secret` |
| Say which programs of a Windows machine people may open | `server` | [7](../plan/README.md#7-programs) | `offer-programs` |
| Give a program an icon of its own, which people know it by | `server` `web` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `give-program-icons` |
| Read the desktops and programs a farm of Windows machines publishes from the farm itself, as it changes | `server` `web` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `read-programs-from-host` |
| Set how a machine is reached: its port, its screen, what may pass | `server` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) | `set-connection-options` |
| Test a machine: is it reached, does signing in work, and if not, which step fails | `web` `server` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) | `test-machine` |
| Say how a machine proves who it is, and accept a changed identity on purpose | `web` `server` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) | `know-machine-identity` |

### Who reaches what

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Allow a group of people to reach a machine or a group of machines | `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `allow-by-group` |
| Show a person only the machines a rule gives them | `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `show-only-allowed` |
| See what a person reaches, and by which rule | `web` `server` | [9](../plan/README.md#9-sessions-as-they-run) | `see-who-reaches` |
| Take people and their groups from the identity provider, and keep none of its own | `server` | [4](../plan/README.md#4-getting-in-and-the-frame) | `take-people-from-provider` |
| Say who administers Away, who reads its records, and who only connects | `server` | [4](../plan/README.md#4-getting-in-and-the-frame) | `name-roles` |
| Allow or forbid the clipboard, in each direction | `server` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) | `limit-clipboard` |
| End a session nobody has used for a set time, with a warning before | `session-view` `gateway` | [9](../plan/README.md#9-sessions-as-they-run) | `end-when-idle` |

### Sessions as they run

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| See who is connected to what right now | `web` `server` `gateway` | [9](../plan/README.md#9-sessions-as-they-run) | `see-live-sessions` |
| End a session, or all of a person's | `web` `server` `gateway` | [9](../plan/README.md#9-sessions-as-they-run) | `end-session` |
| End a person's sessions as soon as their right or their account is gone | `server` `gateway` | [9](../plan/README.md#9-sessions-as-they-run) | `end-on-lost-right` |

### What happened

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Keep a record of every connection: who, which machine, as whom, when, from where, how it ended | `server` | [6](../plan/README.md#6-a-desktop-over-rdp) | `record-connections` |
| Keep a record of every sign-in Away made on a machine for someone | `server` `issuer` | [6](../plan/README.md#6-a-desktop-over-rdp) | `record-sign-ins-made` |
| Keep a record of every change to machines, rules and settings | `server` | [5](../plan/README.md#5-the-register-the-rules-and-home) | `record-changes` |
| Read the records and search them | `web` `server` | [9](../plan/README.md#9-sessions-as-they-run) | `read-records` |
| Send every record on to the organisation's own log system as it is written | `server` | [9](../plan/README.md#9-sessions-as-they-run) | `send-records-on` |
| Keep a record of every file moved and of what the clipboard carried | `server` `gateway` | [6](../plan/README.md#6-a-desktop-over-rdp) | `record-moved-content` |

## The platform

### Installing and configuring

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Install it on Kubernetes or OpenShift with one chart | `chart` | [1](../plan/README.md#1-the-ground) | `install-with-chart` |
| Run with no special rights on the cluster | `chart` | [1](../plan/README.md#1-the-ground) | `run-without-special-rights` |
| Use the identity provider that is there, over OpenID Connect | `server` `chart` | [4](../plan/README.md#4-getting-in-and-the-frame) | `use-own-provider` |
| Say which part of a person's sign-in names their account on the machines | `server` `issuer` | [10](../plan/README.md#10-a-shell-as-oneself), [12](../plan/README.md#12-windows-as-oneself) | `name-account-claim` |
| Set the address it answers on, behind the reverse proxy that is there | `server` `chart` | [1](../plan/README.md#1-the-ground), [4](../plan/README.md#4-getting-in-and-the-frame) | `set-address` |
| Use the cluster's certificates, or one's own | `chart` | [1](../plan/README.md#1-the-ground), [4](../plan/README.md#4-getting-in-and-the-frame) | `use-own-certificates` |
| Choose who issues the sign-ins on a Windows domain's machines: Away's own authority or the organisation's | `issuer` `chart` | [12](../plan/README.md#12-windows-as-oneself), [13](../plan/README.md#13-aways-own-authority) | `connect-domain-authority` |
| Give a domain's controllers the certificates they need, where the domain has no authority of its own | `issuer` `docs` | [13](../plan/README.md#13-aways-own-authority) | `certify-domain-controllers` |
| Have the shells trust Away's sign-in, with one line on each machine | `issuer` `docs` | [10](../plan/README.md#10-a-shell-as-oneself) | `make-shells-trust` |
| Run the part that signs people in on machines apart from the rest, with rights of its own | `issuer` `chart` | [1](../plan/README.md#1-the-ground) | `sign-apart` |
| Keep what has to stay secret encrypted, with the key kept elsewhere | `server` `issuer` `chart` | [4](../plan/README.md#4-getting-in-and-the-frame) | `protect-secrets` |
| Check the whole setup and say what is wrong: the provider, the authority, each kind of machine | `web` `server` `gateway` `issuer` | [15](../plan/README.md#15-status-and-the-check-of-the-setup) | `check-setup` |
| Say whom people turn to, and show it wherever something fails | `web` `server` | [4](../plan/README.md#4-getting-in-and-the-frame) | `name-support-contact` |

### Running it

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Update to a new version in one step, with people told and their desktops back by themselves | `session-view` `gateway` `chart` | [11](../plan/README.md#11-a-desktop-over-vnc) | `update-in-one-step` |
| See whether Away and what it rests on are well | `web` `server` `gateway` `issuer` | [15](../plan/README.md#15-status-and-the-check-of-the-setup) | `see-health` |
| Give the monitoring that is there its measures: sessions, sign-ins, failures, delay | `server` `gateway` `issuer` `chart` | [1](../plan/README.md#1-the-ground), [9](../plan/README.md#9-sessions-as-they-run), [10](../plan/README.md#10-a-shell-as-oneself) | `give-measures` |
| Write what happens as lines a log collector reads | `server` `gateway` `issuer` | [1](../plan/README.md#1-the-ground) | `write-logs` |

### Checking it

| Capability | Carried by | Built in package | Key |
|---|---|---|---|
| Check that an image is the one the project built | `pipelines` `docs` | [1](../plan/README.md#1-the-ground) | `verify-image` |
| See what an image holds: every part, its version, its licence | `pipelines` | [1](../plan/README.md#1-the-ground) | `see-image-contents` |
| Build it from its source with one command | `pipelines` `docs` | [1](../plan/README.md#1-the-ground) | `build-from-source` |
| Run its tests on one's own machine, against a real server of each protocol | `pipelines` `test-servers` | [16](../plan/README.md#16-ready-to-release) | `run-its-tests` |
| Run it with no call to anyone outside: no fonts, scripts, statistics or update checks | `web` `server` `pipelines` | [4](../plan/README.md#4-getting-in-and-the-frame) | `call-nobody-else` |
| See in Away itself which version runs, where its source is and the licences of what it is built from | `web` `server` `pipelines` | [4](../plan/README.md#4-getting-in-and-the-frame) | `see-version-and-licences` |
| Report a security hole in private, and learn what happens next | `docs` | [1](../plan/README.md#1-the-ground) | `report-in-private` |

- *Run it with no call to anyone outside: no fonts, scripts, statistics or update checks*: A test on every change proves it.
<!-- end: carried -->
