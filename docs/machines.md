# The machines and the rules

Away knows the machines, their groups and the rules from files. The files live
where an operator keeps configuration, are reviewed like code, and are applied
as they change. Nothing in the browser changes them.

This version reads the files, tells each person the machines a rule gives
them, and shows those who administer Away what is in force. It opens no
machine yet.

## The files

The files are YAML. A file holds machines, rules, or both, and an installation
has as many files as suit it: one for everything, or one for each department.

```yaml
machines:
  - name: fin-app-02
    kind: windows
    address: fin-app-02.corp.example
    groups: [finance]
    desktop: true
    programs: [Ledger, Payroll, Reports, Mail]

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
| `programs` | the programs of a Windows machine that open in windows of their own, by their names | none |
| `shell` | its shell: `true`, or its port | none |
| `collection` | for a farm of Windows machines that people see as one: the collection this entry stands for | none |
| `served-by` | the machines that serve the collection, by their addresses | none |

A machine offers at least one way in: a desktop, programs, or a shell.

A desktop is reached over RDP on a Windows machine, on port 3389, and over VNC
on a Mac and on a Linux machine, on port 5900. A Linux desktop that speaks RDP
says so, and a way in that listens elsewhere names its port:

```yaml
    desktop: { over: rdp }
    shell: { port: 2222 }
```

A shell is reached over SSH, on port 22.

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

Or keep them in a ConfigMap of one's own, each file under its name, and name
the ConfigMap:

```sh
kubectl --namespace away create configmap away-files --from-file=machines.yaml --from-file=rules.yaml
helm upgrade away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --reuse-values --set filesConfigMap=away-files
```

The server reads every `.yaml` and `.yml` file of the folder and of its
folders, and leaves everything else.

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

## What people and administrators see

- **Home** shows a person what they may open, in the groups of the files:
  each machine that has a desktop or a shell, and after the machines every
  program as an entry of its own, named with its machine. A machine that
  offers programs alone is seen as its programs. A field finds a machine by its
  name, its address or its group, and a program by its name. What is in no
  group is under *other machines*.
- **The order is the files' own.** The files are read in the order of their
  names, and inside a file the machines and their programs keep their place.
  A group comes where a machine first names it.
- **A machine's page**, at `/machines/<name>`, says what the machine offers
  and what it is. A link to it works for everyone the machine is given to.
- **The administration**, for the groups in `administrators`, lists the
  machines and the rules that are in force, each with its file and its line,
  who reaches each machine and by which rule, and since when the set is in
  force.
