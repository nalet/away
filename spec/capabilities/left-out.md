# What is left out

Everything the model marks "not planned", in one place. A capability Away will
not have is in the model all the same, so that leaving it out is a decision
someone can see and overrule, and not something forgotten. Its note says why.

To overrule one, change its cell in [capabilities.csv](./capabilities.csv) from
`no` to `later` or `v1`.

<!-- generated: left-out -->
## Not planned at all

13 capabilities.

| Left out | For | Why | Area | Key |
|---|---|---|---|---|
| Open other kinds of target: Telnet, a serial console, a database, a web page | Away itself | Away is for desktops and shells | [Opening a machine](./session.md#opening-a-machine) | `open-other-kinds` |
| Use one's own camera on the machine | RDP, VNC | a call is made on one's own device | [Sound and devices](./session.md#sound-and-devices) | `use-camera` |
| Use a USB device or a smart card of one's own on the machine | RDP, VNC | a browser hands neither to a page | [Sound and devices](./session.md#sound-and-devices) | `use-own-devices` |
| Forward a port through the shell's connection | SSH | Away carries sessions and no other traffic | [The shell](./session.md#the-shell) | `forward-port` |
| Sign in through an identity provider that speaks SAML only | Away itself | OpenID Connect is the one way in | [Signing in](./app.md#signing-in) | `sign-in-with-saml` |
| Go on from the machine to a further one with nothing asked | RDP, SSH | the further machine is opened from Away: every connection starts there | [Signing in](./app.md#signing-in) | `go-on-without-password` |
| Use an installed app instead of the browser, on a computer or a phone | Away itself | the browser is the one way in | [The frame](./app.md#the-frame) | `use-installed-app` |
| Invite someone who has no account | Away itself | everyone in a session is known to the identity provider | [Working together](./app.md#working-together) | `invite-without-account` |
| Change a kept password on the machine by itself from time to time | Away itself | Away keeps the secrets it needs and manages no others | [The register of machines](./admin.md#the-register-of-machines) | `change-kept-secret` |
| Make, copy or hand out fresh desktops as people ask for them | Away itself | Away reaches machines that exist | [The register of machines](./admin.md#the-register-of-machines) | `make-desktops` |
| Reach a machine through a helper installed on it | Away itself | a machine is reached the way it already offers | [The register of machines](./admin.md#the-register-of-machines) | `install-helper-on-machine` |
| Bring an identity provider along for those who have none | Away itself | Away asks an identity provider and is none | [Installing and configuring](./platform.md#installing-and-configuring) | `bring-provider-along` |
| Carry other traffic to the machines' network, as a private network would | Away itself | Away carries sessions and no other traffic | [Installing and configuring](./platform.md#installing-and-configuring) | `carry-other-traffic` |

## Not planned for one kind of connection

11 capabilities that are planned for another.

| Left out | For | Why | Area | Key |
|---|---|---|---|---|
| Play a video on the machine smoothly | VNC (RDP later) | the browser plays the machine's own video stream; VNC has none | [The picture](./session.md#the-picture) | `play-video-smoothly` |
| Spread a machine over several of one's own screens | VNC (RDP later) | VNC shows the machine's own screens as they are | [The picture](./session.md#the-picture) | `several-screens` |
| Copy and paste a picture, or text with its formatting | VNC (RDP later) | VNC carries text alone | [Clipboard, files and printing](./session.md#clipboard-files-and-printing) | `copy-paste-rich` |
| Copy files on one side and paste them on the other | VNC (RDP later) | VNC carries text alone | [Clipboard, files and printing](./session.md#clipboard-files-and-printing) | `copy-paste-files` |
| Move files between one's own device and the machine | VNC (RDP later, SSH later) | a shell: in a panel beside it; a Windows machine: as a drive the machine sees; VNC moves no files | [Clipboard, files and printing](./session.md#clipboard-files-and-printing) | `move-files` |
| Drop a file onto a session to send it to the machine | VNC (RDP later, SSH later) | VNC moves no files | [Clipboard, files and printing](./session.md#clipboard-files-and-printing) | `drop-file` |
| Print from the machine to a printer or a file of one's own | VNC (RDP later) | VNC carries no printing | [Clipboard, files and printing](./session.md#clipboard-files-and-printing) | `print-to-own-device` |
| Hear the machine's sound | VNC (RDP later) | VNC carries no sound | [Sound and devices](./session.md#sound-and-devices) | `hear-sound` |
| Speak to the machine through one's own microphone | VNC (RDP later) | VNC carries no sound | [Sound and devices](./session.md#sound-and-devices) | `use-microphone` |
| Be signed in on the machine as oneself, with nothing asked | VNC (RDP v1, SSH v1) | a Windows machine in a domain, and a shell that trusts Away; an ordinary account is enough; VNC knows no identity | [Signing in](./app.md#signing-in) | `be-oneself-on-machine` |
| Find a moment in a recording by what was typed or shown | RDP, VNC (SSH later) | a shell is recorded as text, a desktop as pictures | [What happened](./admin.md#what-happened) | `search-recording` |

## Expectations the plan does not meet

8 expectations.

| Expected | For the goal | Note |
|---|---|---|
| One's own camera, USB devices and smart cards on the machine | [Work on a machine that is somewhere else](./expectations.md#work-on-a-machine-that-is-somewhere-else) | — |
| Other kinds of target: a serial console, a database, a web page | [Work on a machine that is somewhere else](./expectations.md#work-on-a-machine-that-is-somewhere-else) | — |
| Signing in through SAML | [Get in without another password](./expectations.md#get-in-without-another-password) | — |
| Invite someone who has no account | [Look at a machine together](./expectations.md#look-at-a-machine-together) | — |
| A fresh desktop made for whoever asks | [Know the machines](./expectations.md#know-the-machines) | — |
| An identity provider comes along for those who have none | [Run it oneself](./expectations.md#run-it-oneself) | — |
| A private network to the machines, for other traffic too | [Run it oneself](./expectations.md#run-it-oneself) | — |
| Kept passwords are changed on the machines from time to time | [Run it oneself](./expectations.md#run-it-oneself) | — |
<!-- end: left-out -->
