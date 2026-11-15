# The machines and the rules

Away knows the machines, their groups and the rules from files. The files live
where an operator keeps configuration, are reviewed like code, and are applied
as they change. Nothing in the browser changes them.

This version reads the files, tells each person the machines a rule gives
them, shows those who administer Away what is in force, and opens Windows
desktops and the programs of Windows machines: those of one machine, and
those of a farm of machines ([opening a desktop](#opening-a-desktop),
[opening a program](#opening-a-program)). It doesn't open shells or other
desktops yet.

## The files

The files are YAML. A file holds machines, rules and farms, any of them, and
an installation has as many files as suit it: one for everything, or one for
each department.

```yaml
machines:
  - name: fin-app-02
    kind: windows
    address: fin-app-02.corp.example
    groups: [finance]
    desktop: true
    programs:
      - name: Ledger
        icon: icons/ledger.png
      - Payroll
      - Reports
      - Mail

  - name: build-01
    kind: linux
    address: build-01.corp.example
    groups: [engineering]
    shell: true

rules:
  - name: finance-desktops
    who: [finance]
    reaches: [finance]

  - name: engineers-build
    who: [engineering]
    reaches: [build-01]
```

### A machine

| Part | Means | |
|---|---|---|
| `name` | what people know the machine by, and what its page's address ends in. No two machines share one, however their letters are written. It takes letters, digits, spaces and `- _ . ( ) + &` | needed |
| `kind` | what it is: `windows`, `linux` or `mac` | needed |
| `address` | where it is reached: a name of the network, or a number | needed |
| `groups` | the groups of machines it is in. A group is there as soon as a machine names it | none |
| `desktop` | its desktop: `true`, or its port and what it is reached over | none |
| `programs` | the programs of a Windows machine that open in windows of their own: each its name, or its name and its icon | none |
| `shell` | its shell: `true`, or its port | none |
| `collection` | for a farm of Windows machines that people see as one: the collection this entry stands for. Its address is then the farm's broker | none |
| `served-by` | the machines that serve the collection, by their addresses. One that opens its desktop on another port than the broker names it after a colon, as `rdsh-02.corp.example:3390`. Away goes on to no other machine for the collection | none |
| `signs-in` | how people are signed in on a Windows machine: `typed`, with a name and a password they type, or `themselves`, with nothing typed | `typed` |
| `clipboard` | which ways text passes between the person's clipboard and the machine's, on a Windows machine: `both`, `to-the-machine`, `from-the-machine` or `none` | `both` |

A machine offers at least one way in: a desktop, programs, or a shell.

A desktop is reached over RDP on a Windows machine, on port 3389, and over VNC
on a Mac and on a Linux machine, on port 5900. A Linux desktop that speaks RDP
says so, and a way in that listens elsewhere names its port:

```yaml
    desktop: { over: rdp }
    shell: { port: 2222 }
```

A shell is reached over SSH, on port 22.

A program is known by its name. With an icon it is known by its picture too:
the icon names a PNG picture of the folder, by where it is in the folder. A
picture is 256 KiB at most. Away serves the pictures itself, to people who are
signed in.

```yaml
    programs:
      - name: Ledger
        icon: icons/ledger.png
      - Payroll
```

### A rule

| Part | Means | |
|---|---|---|
| `name` | what the rule is called. No two rules share one | needed |
| `who` | the groups of people it is for, as the sign-in service names them | needed |
| `reaches` | what they reach: machines and groups of machines, by name | needed |

Someone reaches a machine when a rule that names one of their groups reaches
the machine or a group the machine is in. Nobody reaches a machine that no
rule gives them: they don't see it, and its address says that there is no
machine of that name for them.

A group of people is the whole name the sign-in service gives it, such as
`finance` or `/departments/finance`. A name in `reaches` is a machine or a
group of machines; one that is both is refused, since the rule could mean
either.

### A farm

A farm of Windows machines publishes desktops and programs through a broker,
and its web access says what there is, in a feed. Name the feed, and Away asks
it: a program the farm publishes tomorrow shows tomorrow, and no file changes.

```yaml
farms:
  - name: Work Resources
    feed: https://broker.corp.example/RDWeb/Feed/webfeed.aspx
    groups: [published]
    served-by: [rdsh-01.corp.example, rdsh-02.corp.example]

rules:
  - name: staff-published
    who: [staff]
    reaches: [published]
```

A farm says `signs-in: themselves` as a machine does, for everything it
publishes.

| Part | Means | |
|---|---|---|
| `name` | what the farm is called. Its programs show as the programs of a machine of this name, so no machine and no other farm shares it | needed |
| `feed` | the address of the farm's feed, over HTTPS | needed |
| `groups` | the groups of machines that everything the farm publishes is in | none |
| `served-by` | the machines that serve what the farm publishes, by their addresses. Away goes on to no other machine for it, so a farm names them unless its broker serves everything itself | none |
| `clipboard` | which ways text passes between the person's clipboard and the machines', for everything the farm publishes: `both`, `to-the-machine`, `from-the-machine` or `none` | `both` |

- **A desktop the farm publishes is a machine**, by the name the farm gives
  it. It is opened through the farm's broker
  ([opening a desktop](#opening-a-desktop)).
- **The farm's programs are the programs of one machine**, which has the
  farm's name. Each keeps the icon the farm gives it, and is started as the
  farm publishes it ([opening a program](#opening-a-program)).
- **The rules give them like everything else.** A rule reaches the farm by
  its name, or by a group the farm names.
- **Away asks every five minutes.** What the farm changed shows the next time
  a page asks, and the server writes a line: `a farm's feed was read`, with
  how many desktops and programs.
- **A feed that can't be read changes nothing.** What the farm published
  before stays, the administration says why the feed could not be read, and
  the server writes a line:
  `a farm's feed could not be read, and what it published before stays`.
  What a farm publishes is held in memory: after the server starts, a farm
  shows nothing until its feed was read once.
- **A machine of the files keeps its name.** A desktop that the farm
  publishes under the name of a machine in the files is left out, and the
  administration names it.
- **What a farm publishes shows where the farm is written**: after the
  machines that are written before it in the files' order, and before those
  after it.

Away reads the feeds as an account of its own, which the installation names
([installing Away](./install.md#the-farms-feeds)). A farm shows that account
what it would show it in a browser, so:

- the account is in the groups the farm publishes to;
- what the account sees is what Away offers, to the people its rules name.
  Whom the farm lets open a program stays the farm's to say;
- the account signs in at the farm's web access, with NTLM inside the secured
  connection, and nowhere else. Away opens nothing with it.

Without the account, a farm in the files shows nothing, and the administration
says so. A web access that sends its readers to another address to sign in is
not followed: name the feed where the farm itself signs its readers in.

### What the files don't take

One document a file, and plain YAML: no references (`&`, `*`), no tags (`!`).
A part that a machine or a rule doesn't have is refused, so a slip of the
keys doesn't pass for a setting.

## Giving Away the files

The chart takes the files as values, each by its name. A name may have folders:

```sh
helm upgrade away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --reuse-values \
  --set-file 'files.machines/finance\.yaml=machines/finance.yaml' \
  --set-file 'files.rules/access\.yaml=rules/access.yaml'
```

The pictures go the same way, each by its name and in Base64:

```sh
  --set "pictures.icons/ledger\.png=$(base64 < icons/ledger.png | tr -d '\n')"
```

Or keep them in a ConfigMap of one's own, each file and each picture under its
name, and name the ConfigMap:

```sh
kubectl --namespace away create configmap away-files --from-file=machines.yaml --from-file=rules.yaml
helm upgrade away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --reuse-values --set filesConfigMap=away-files
```

The server reads every `.yaml` and `.yml` file of the folder and of its
folders, takes every `.png` picture for a program to name as its icon, and
leaves everything else.

## A change is applied whole or not at all

The server looks at the files every few seconds. When they changed, it reads
the whole set and checks it as one: every machine, every rule, and everything
a rule names. A cluster brings a changed ConfigMap to the server within about
a minute.

- **A set that holds up is applied.** People see what the rules now give them
  the next time their page asks, and the server writes a line:
  `the files were applied`, with how many files, machines and rules.
- **A set with one thing wrong is refused whole.** What was in force stays in
  force. The administration shows the file, the line and the reason until the
  files are put right, and the server writes a line:
  `a change to the files was refused, and what was in force stays`.

```
A change to machines/finance.yaml was not applied on 14 November 2026 at 09:10.
In line 14, “prot” is not something a machine has. What was in force before stays.
```

When the files are wrong as the server starts, nothing is in force until they
are put right.

With a database ([installing Away](./install.md#the-database)), each of the two
is kept as a record, and so is what a farm publishes whenever it is something
else than before. A record is only ever added: the database itself refuses to
change or to remove one. Each is also a line on the server's output at the
moment it is written, `record`, with its kind and its details, so that the
organisation's log system holds a copy. A server that starts anew and finds
the files as they were adds no record.

## Opening a desktop

A click on a machine that offers its desktop and nothing else opens it. A
machine that offers more is opened from its own page, where each way in is.

- **The steps show as they happen**: reaching the machine, checking that it is
  the one Away knows, signing in, its first picture. A step that fails says
  why, with the technical reason behind *details*.
- **The machine is known by what it showed the first time.** A machine that
  shows another identity later is not connected to.
- **The person types a name and a password** when the machine asks. Away
  passes them on to the machine and keeps them nowhere. A Windows machine
  takes them before it shows anything, so a wrong one is asked for again.
- **Or nothing is typed.** On a machine whose entry says
  `signs-in: themselves`, the person is signed in as themselves. Away asks the
  organisation's own authority for a certificate that lasts five minutes,
  proves the person to the domain with it, and hands the machine a smart card
  that exists in Away's memory alone. The machine signs the person in as it
  does with a card at a desk, so they reach file shares and printers as
  themselves. Such a machine is named by the name its domain knows it by, and
  by no number, and so are the machines that serve a farm. A person Away
  signs in as themselves on no machine, such as one of the directory's
  administrators, is asked for a name and a password on the same machine, and
  told that it is in place of that: such a machine needs no second entry for
  them. It needs the installation to name the domain, its
  authority and the account Away asks the authority as
  ([signing in as oneself](./install.md#signing-in-as-oneself-on-a-windows-domain)).
- **A farm's desktop is opened through its broker.** Away tells the broker
  which collection is wanted and signs the person in there. The broker says
  which machine serves the session, and Away goes on to that machine, where
  the same name and password sign in. The person sees the same steps and
  types once.
- **A broker's word alone leads Away to no machine.** Away goes on only to a
  machine the files name for the collection: one under `served-by`, or the
  entry's own address, where the broker serves the collection itself. To any
  other machine it doesn't go, and says so. The broker and each machine behind
  it is known by what it showed the first time, and the record of the open
  desktop names the machine that served it.
- **The machine has the whole window.** Every key, every click and the wheel go
  to it. A small handle at the top edge opens the session's bar, and so does
  Ctrl + Alt + Shift, pressed and let go with no other key. The bar sends the
  key combinations a browser keeps for itself (Ctrl + Alt + Del, Alt + Tab,
  Alt + F4, the Windows key and Ctrl + W), holds the clipboard's panel, fills
  the screen and ends the session. While a session fills the screen, a browser that allows it gives
  every key to the machine.
- **A Windows desktop opens at the window's size, in the screen's own dots.**
  On a screen with more dots than a page counts, the machine is asked for
  that many and draws larger by as much, so text has the size it has at the
  machine and is sharp. A very large window on such a screen gets a desktop of
  3840 by 2160 dots at most, drawn a little less large.
- **Text copied on one side is pasted on the other**, on a Windows machine,
  each way where the machine's entry lets it. In a browser that lets a page
  read its clipboard, which it asks once, this needs nothing more: what the
  person copied is told to the machine when its screen gets the focus, and
  what is copied on the machine is in the person's clipboard. In a browser
  that doesn't, the bar's clipboard panel has a field for text to the machine
  and one for text from it. Text passes, up to about a quarter of a million
  characters; pictures and files don't.
- **A Windows desktop follows its window.** Once the window has rested at
  another size, the machine is told and offers its desktop anew at that size,
  so nothing is scaled and text keeps its size. A machine that doesn't take a
  new size keeps the one it opened with, and so do the windows of a program.
- **A key writes what it writes at home.** The session is given the layout of
  the person's keyboard: where the browser says what the keys write, by that;
  elsewhere by the language the browser is set to.
- **Ending a session leaves the desktop running** on the machine, for the
  account to come back to.
- **A desktop whose connection drops is opened again by itself**: at once,
  and then at longer intervals. After two seconds without the machine a card
  over the last picture says so, with a button to try now and one to leave.
  Where the person is signed in as themselves, the desktop is back as it was
  left with nothing typed; where people type, the machine asks again. When
  Away itself is updated, the gateway tells every open session before it
  stops, and the card says that the desktop comes back by itself.
- **No machine is opened without a record.** That a desktop was asked for,
  that it is open, and how it ended or failed is each kept as a record, with
  who it was and which machine. A sign-in that Away made for a person is a
  record too, kept before they are signed in: as which account, and until
  when. Without a database, no machine is opened.

It needs a database ([installing Away](./install.md#the-database)), and the
gateway has to reach the machines
([what opening a machine needs](./install.md#opening-machines)).

## Opening a program

A click on a program opens it: on home, on the programs' page, and on its
machine's page. The steps, the machine's question for a name and a password,
and the records are those of a desktop.

- **A program opens in windows of its own, and no desktop is around them.**
  The windows are where the machine has them, on Away's ground. Keys, clicks
  and the wheel go to the window they are over.
- **The session's bar lists the program's windows.** A click on one brings it
  to the front, and each can be closed there. Dialogues and menus belong to
  their window and are in no list.
- **Closing the program's last window ends the session**, and the person is
  back on the machine's page. Ending the session from the bar leaves the
  program running on the machine, for the account to come back to.
- **Programs of one machine share one sign-in there.** A program that is
  opened while another of the same machine still runs for the account shows
  beside it, and the bar lists the windows of both.
- **The machine decides which programs it starts.** A Windows machine starts
  the programs that are published on it for remote use, each under the name
  it is published as; a farm's feed says those names itself. A program that
  the machine doesn't let be started fails at its step, with the machine's
  reason behind *details*.

In the files a program is its name, or its name with its icon and with what
the machine is asked to start:

```yaml
    programs:
      - Ledger
      - name: Payroll
        icon: icons/payroll.png
      - name: Reports
        run: '||reports-2026'
```

| Part | Means | |
|---|---|---|
| `name` | what people know the program by | needed |
| `icon` | its picture: a PNG among the files, named by where it is in the folder | none |
| `run` | what the machine is asked to start: the name the program is published under, with `\|\|` before it, or the path of a program on the machine | `\|\|` and the program's name |

## What people and administrators see

- **Programs** is a page of every program a person may open, each an entry of
  its own, named with its machine, with a field that finds a program by its
  name.
- **Machines** is a page of the machines a person may reach, in the groups of
  the files, with a field that finds a machine by its name, its address or its
  group. A machine in no group is under *other machines*.
- **Home** is the place for what a person starred: every tile has a star, on
  the programs' page, on the machines' page and on a machine's own. What is
  starred is kept with the person, so it is the same in every browser they
  sign in with, and it shows for as long as a rule gives it to them. Someone
  who starred nothing is told where the programs and the machines are.
- **Without a database** nothing is kept
  ([installing Away](./install.md#the-database)). There are no stars then,
  and home shows everything: the machines that have a desktop or a shell, and
  after them every program.
- **The order is the files' own.** The files are read in the order of their
  names, and inside a file the machines and their programs keep their place.
  A group comes where a machine first names it.
- **A machine's page**, at `/machines/<name>`, says what the machine offers
  and what it is. A link to it works for everyone the machine is given to.
- **The administration**, for the groups in `administrators`, lists the
  machines and the rules that are in force, each with its file and its line,
  who reaches each machine and by which rule, and since when the set is in
  force. It lists the farms as well: each with its feed, what it publishes,
  when its feed was last read, and what kept it from being read.
