# In a session

What a person does once a machine is open: its desktop, one of its programs or
its shell. Most of it depends on the protocol the machine speaks, so each
capability stands somewhere for RDP, for VNC and for SSH.
[How to read the tables](./README.md#how-to-read-it).

<!-- generated: areas -->
## Opening a machine

Getting a machine's desktop, one of its programs or its shell on the screen, and coming back to it. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | RDP | VNC | SSH | Away | Key |
|---|---|---|---|---|---|---|
| Open a machine's desktop | person | partial | built | — | — | `open-desktop` |
| Open one program of a Windows machine, without its desktop | person | built | — | — | — | `open-program` |
| Switch between the programs open on a machine, each in a window of its own | person | v1 | — | — | — | `switch-programs` |
| See a machine's open windows in a list beside them, and bring one to the front, also one made small | person | partial | — | — | — | `list-open-windows` |
| Open a shell on a machine | person | — | — | v1 | — | `open-shell` |
| Keep several machines open at once and move between them | person | v1 | v1 | v1 | — | `open-several` |
| Open a session in a browser tab or window of its own | person | v1 | v1 | v1 | — | `open-in-own-tab` |
| See how far a connection has come, and which step failed and why | person | built | built | v1 | — | `see-connecting-steps` |
| Come back to a session by itself after the connection dropped | system | v1 | v1 | v1 | — | `reconnect-by-itself` |
| Leave a session running while the browser is closed, and pick it up again | person | v1 | v1 | later | — | `keep-while-away` |
| Choose between leaving a desktop running and signing out of the machine | person | no | — | — | — | `leave-or-sign-out` |
| See why a session ended, and open it again from there | person | v1 | v1 | v1 | — | `see-why-ended` |
| See that a desktop is in use by someone else, where opening it would take it from them or show their screen | person | v1 | v1 | — | — | `see-in-use` |
| Open a machine straight into a given program or command | admin | later | — | later | — | `start-with-command` |
| See the programs of several machines side by side, in one list of windows | person | later | — | — | — | `programs-of-several-machines` |
| Use the small icons a program puts beside the machine's clock | person | later | — | — | — | `use-tray-icons` |
| Reach a farm of Windows machines through its broker and land on the right one | system | built | — | — | — | `follow-broker` |
| Take the licence that a machine for many people hands every client | system | built | — | — | — | `take-licence` |
| Reach a shell that is only reachable through another machine | system | — | — | later | — | `jump-through` |
| Open other kinds of target: Telnet, a serial console, a database, a web page | person | — | — | — | no | `open-other-kinds` |

- *Open one program of a Windows machine, without its desktop*: a program the machine publishes for remote use.
- *Switch between the programs open on a machine, each in a window of its own*: one sign-in on the machine holds them all.
- *See a machine's open windows in a list beside them, and bring one to the front, also one made small*: programs bring no task bar of the machine's, so the list is how a window that was made small is reached.
- *See how far a connection has come, and which step failed and why*: reaching the machine, checking who it is, signing in, the first picture.
- *Come back to a session by itself after the connection dropped*: a desktop goes on at its machine meanwhile; a shell is held by Away for a few minutes, while its page stays open.
- *Leave a session running while the browser is closed, and pick it up again*: a desktop goes on running on its machine; a shell has to be held by Away.
- *Choose between leaving a desktop running and signing out of the machine*: Away only ends the connection, so that a desktop is taken along to another device; signing out is done at the machine.
- *See why a session ended, and open it again from there*: nobody used it, an administrator ended it, the right to it is gone, the machine closed it, it was opened elsewhere.
- *See that a desktop is in use by someone else, where opening it would take it from them or show their screen*: a desktop with one screen for everyone, or one account that several people are signed in with.
- *See the programs of several machines side by side, in one list of windows*: in the first release each machine's programs are a session of their own.
- *Use the small icons a program puts beside the machine's clock*: for a program that hides its window there.
- *Reach a farm of Windows machines through its broker and land on the right one*: a machine's file names its collection and the machines that serve it; Away goes where the broker sends it, and to no machine the file doesn't name.
- *Take the licence that a machine for many people hands every client*: a Windows machine that serves desktops or programs to many people asks every client to take one before it shows anything.
- *Open other kinds of target: Telnet, a serial console, a database, a web page*: Away is for desktops and shells.

## The picture

How the machine's screen reaches one's own: its size, its sharpness, how smoothly it moves, several screens. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | RDP | VNC | SSH | Key |
|---|---|---|---|---|---|
| Fit the machine's screen to the window, and follow when its size changes | person | v1 | built | v1 | `fit-to-window` |
| Fill the whole screen with a session | person | v1 | built | v1 | `fill-screen` |
| Show a sharp picture on a high-density screen | system | v1 | later | v1 | `sharp-on-dense-screen` |
| Show a screen at rest exactly, pixel for pixel | system | v1 | built | — | `exact-at-rest` |
| Give up detail while the picture moves or the line is slow, and restore it after | system | v1 | v1 | — | `adapt-to-line` |
| Choose between a sharper and a faster picture by hand | person | later | later | — | `choose-sharper-or-faster` |
| Show the desktop as it looks at the machine: its background, smooth fonts, its effects | system | v1 | — | — | `desktop-as-at-machine` |
| Draw the pointer on one's own device, so that it moves without delay | system | v1 | v1 | — | `pointer-without-delay` |
| Play a video on the machine smoothly | system | later | no | — | `play-video-smoothly` |
| Show each change of the picture whole, never in pieces | system | later | later | — | `show-whole-pictures` |
| Take a Windows machine's picture whole, as the machine composes it, at the pace the browser shows it | system | later | no | — | `machine-made-pictures` |
| Leave out a machine's animations and background, for a faster picture | person | later | no | — | `leave-out-effects` |
| Spread a machine over several of one's own screens | person | later | no | — | `several-screens` |
| Choose which of a machine's screens to see | person | — | later | — | `choose-machine-screen` |

- *Fit the machine's screen to the window, and follow when its size changes*: a Windows machine and a shell take the window's size; other desktops are scaled to it.
- *Fill the whole screen with a session*: where the browser can; a phone's browser may not.
- *Show a sharp picture on a high-density screen*: a desktop over VNC has the size its machine gives it.
- *Play a video on the machine smoothly*: the browser plays the machine's own video stream; VNC has none.
- *Take a Windows machine's picture whole, as the machine composes it, at the pace the browser shows it*: VNC sends what changed as rectangles alone.
- *Leave out a machine's animations and background, for a faster picture*: VNC has no way to ask a machine for it.
- *Spread a machine over several of one's own screens*: VNC shows the machine's own screens as they are.
- *Choose which of a machine's screens to see*: for a machine with several screens of its own.

## Keyboard and pointer

Typing, shortcuts, the mouse and the trackpad, as they arrive on the machine. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | RDP | VNC | SSH | Key |
|---|---|---|---|---|---|
| Type with one's own keyboard layout, whatever the machine's is | person | partial | built | v1 | `type-in-own-layout` |
| Send the key combinations a browser or a device keeps for itself, from a menu | person | v1 | built | v1 | `send-kept-keys` |
| Give every key to the machine while a session fills the screen | person | v1 | built | v1 | `give-all-keys` |
| Use a Mac keyboard on a Windows machine and the reverse, with the keys where the hands expect them | person | v1 | v1 | — | `map-modifier-keys` |
| Use every mouse button and the wheel, in both directions | person | v1 | partial | v1 | `use-mouse-fully` |
| Scroll with a trackpad as smoothly, as fast and in the direction one is used to | person | v1 | partial | v1 | `scroll-as-used-to` |
| Type the clipboard's text into the machine key by key | person | later | v1 | — | `type-clipboard` |
| Type in a language whose characters are composed of several keys | person | later | later | v1 | `compose-characters` |
| Hold the pointer inside the session for a program that needs it | person | later | later | — | `hold-pointer` |

- *Send the key combinations a browser or a device keeps for itself, from a menu*: Ctrl+Alt+Del, Alt+Tab, the Windows key, Ctrl+W.
- *Give every key to the machine while a session fills the screen*: where the browser allows it.
- *Use every mouse button and the wheel, in both directions*: in a shell, for the programs that take the mouse.
- *Scroll with a trackpad as smoothly, as fast and in the direction one is used to*: whatever the machine's own system makes of a wheel.
- *Type the clipboard's text into the machine key by key*: for a sign-in screen or a console that takes no clipboard.

## Clipboard, files and printing

What passes between one's own device and the machine. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | RDP | VNC | SSH | Key |
|---|---|---|---|---|---|
| Copy text on one side and paste it on the other | person | v1 | partial | v1 | `copy-paste-text` |
| Copy and paste a picture, or text with its formatting | person | built | no | — | `copy-paste-rich` |
| Copy files on one side and paste them on the other | person | partial | no | — | `copy-paste-files` |
| Move files between one's own device and the machine | person | later | no | later | `move-files` |
| Drop a file onto a session to send it to the machine | person | built | no | later | `drop-file` |
| Print from the machine to a printer or a file of one's own | person | later | no | — | `print-to-own-device` |

- *Copy and paste a picture, or text with its formatting*: VNC carries text alone.
- *Copy files on one side and paste them on the other*: one's own files are pasted on the machine, folders later; the machine's are downloaded; VNC carries text alone.
- *Move files between one's own device and the machine*: a shell: in a panel beside it; a Windows machine: as a drive the machine sees; VNC moves no files.
- *Drop a file onto a session to send it to the machine*: VNC moves no files.
- *Print from the machine to a printer or a file of one's own*: VNC carries no printing.

## Sound and devices

Hearing the machine, and what of one's own device the machine may use. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | RDP | VNC | Key |
|---|---|---|---|---|
| Hear the machine's sound | person | later | no | `hear-sound` |
| Speak to the machine through one's own microphone | person | later | no | `use-microphone` |
| Use one's own camera on the machine | person | no | no | `use-camera` |
| Use a USB device or a smart card of one's own on the machine | person | no | no | `use-own-devices` |

- *Hear the machine's sound*: VNC carries no sound.
- *Speak to the machine through one's own microphone*: VNC carries no sound.
- *Use one's own camera on the machine*: a call is made on one's own device.
- *Use a USB device or a smart card of one's own on the machine*: a browser hands neither to a page.

## The shell

What a shell needs that a picture does not: text, colours, its size, what scrolled by. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | SSH | Key |
|---|---|---|---|
| Run the programs that take the whole shell: editors, monitors, with colours and the mouse | system | v1 | `run-full-screen-programs` |
| Scroll back through what the shell showed, and search it | person | v1 | `scroll-back` |
| Select the shell's text with the mouse or the keys, and copy it | person | v1 | `select-shell-text` |
| Set the shell's text size, its font and its colours | person | v1 | `set-shell-look` |
| Open a web address the shell shows | person | later | `open-shell-link` |
| Open a further shell on the same machine, beside the first | person | later | `split-shell` |
| Forward a port through the shell's connection | person | no | `forward-port` |

- *Forward a port through the shell's connection*: Away carries sessions and no other traffic.

## On a phone or a tablet

Working without a keyboard and a mouse: touch, the keys a phone lacks, a small screen. For the goal [Work on a machine that is somewhere else](../goals/README.md#work-on-a-machine-that-is-somewhere-else).

| Capability | Who | RDP | VNC | SSH | Away | Key |
|---|---|---|---|---|---|---|
| Find a machine and open it on a phone, with every page fitted to it | person | — | — | — | built | `use-on-phone` |
| Work by touch: tap to click, hold for the other button, two fingers to scroll | person | partial | partial | v1 | — | `work-by-touch` |
| Zoom into the picture with two fingers and move around it | person | partial | partial | v1 | — | `zoom-by-touch` |
| Use the keys a phone's keyboard lacks: Ctrl, Alt, Esc, Tab, the arrows, the function keys | person | partial | partial | v1 | — | `use-extra-keys` |
| Move the pointer as on a trackpad, for small targets | person | partial | partial | — | — | `touch-as-trackpad` |
| Send touches to the machine as touches | person | later | — | — | — | `send-real-touch` |
| Add Away to a phone's home screen and open it like an app | person | — | — | — | later | `add-to-home-screen` |

- *Work by touch: tap to click, hold for the other button, two fingers to scroll*: a desktop is worked as on a trackpad; tried with a browser's stand-in for fingers, not yet on a phone.
- *Zoom into the picture with two fingers and move around it*: the part that shows follows the pointer; tried with a browser's stand-in for fingers, not yet on a phone.
- *Use the keys a phone's keyboard lacks: Ctrl, Alt, Esc, Tab, the arrows, the function keys*: a desktop has them, with the phone's keyboard; tried in a browser on a computer, not yet on a phone.
- *Move the pointer as on a trackpad, for small targets*: how a desktop is worked by touch; tried with a browser's stand-in for fingers, not yet on a phone.
- *Send touches to the machine as touches*: for a Windows machine's own touch gestures.
<!-- end: areas -->
