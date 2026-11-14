# 15 Administration: status

Status is for whoever runs Away. It says whether Away and what it rests on are
well, and whether signing in works on each kind of machine. It is the last tab
of the administration, and only an administrator has it; a reader of the
records doesn't.

It draws what an administrator sees of
[Running it](../capabilities/platform.md#running-it) and
[Installing and configuring](../capabilities/platform.md#installing-and-configuring):
the health of the parts and the check of the setup. A refused change of the
files belongs to [The register of machines](../capabilities/admin.md#the-register-of-machines),
and the version to [Checking it](../capabilities/platform.md#checking-it).

## 15.1 Away's parts

![The status tab of the administration. A table lists five parts, each with a green mark that says well and a sentence about what it last did. Under it a second table, the check of the setup, has a row for each kind of machine. A line at the foot gives the version and whom people are told to turn to.](img/admin-status-computer.png)

**Figure 15.1** Status, with every part well.

1. **When the parts were asked.** The page asks each part itself when it opens,
   and again every minute while it is open.
2. **A part**: its name, and under it what it is.
3. **Its state**, in a word with a mark, and **what it last did**, with the
   time.
4. **The check of the setup** (15.2).
5. **Check again** runs it now.
6. **A kind of machine**: where signing in was tried, what was tried, and how
   it went.
7. **The foot**: the version that runs, and whom people are told to turn to.

| Part | What it is | It is well when |
|---|---|---|
| the sign-in service | where people sign in to Away, at the address the configuration names | it answers |
| signing in on machines | the part of Away that makes the sign-ins on machines; it runs apart from the rest, with rights of its own | it answers the rest of Away |
| the certificate authority | issues what signs a person in on a Windows machine in a domain | it issued a certificate for a test the last time Away asked, which is every ten minutes |
| the records | Away's records of every connection, sign-in and change | the last record was written and, where records are put out for a log system, put out |
| the files | the files that hold the machines, the groups and the rules | the last change was applied; a change that was refused shows here and on the machines' tab ([chapter 12](./admin-machines.md)) |

- A part that this installation doesn't have is not listed: without a Windows
  domain there is no certificate authority, and without a log system the
  records say nothing of putting out.
- Which authority issues is the configuration's choice: Away's own, or the
  organisation's. The row is the same for either.
- A state is never shown by colour alone: it has a mark and a word, “well” or
  “not answering”.
- The same facts are at an address for health checks and as measures, for the
  monitoring that is there. Whoever runs Away learns of a failure from that
  monitoring first, and looks here next. Those have no screen.
- The foot's second half is what the configuration says. Where it names nobody,
  the line says so: “Nobody is named for people to turn to.”

## 15.2 The check of the setup

Parts 4 to 6 of figure 15.1.

- The check has a row for each kind of machine the files hold: here a Windows
  machine in a domain, a shell, and a desktop with a kept account.
- For each kind it takes one machine and signs in there, as the person who
  asked for the check or with the kept account, and signs out again. It opens
  no picture.
- Check again shows a loader in the button while the check runs, and each row
  changes as its result comes.
- After every change of the files the check runs by itself, as far as it can
  without a person: it reaches a machine of each kind and checks that it is the
  machine Away knows. A row then keeps its last result for signing in, with its
  time.
- A row that failed says which step failed and why, in the words a person sees
  when a connection fails ([chapter 5](./opening.md)). Testing one machine is
  on its own page ([chapter 12](./admin-machines.md)).

## 15.3 When a part fails

![The same tab with a red banner at its top: nobody can be signed in on a Windows machine as themselves right now. In the table the certificate authority's row is marked red and says not answering. In the check of the setup the row for a Windows machine in a domain failed.](img/admin-status-failing-computer.png)

**Figure 15.2** Status, while the certificate authority doesn't answer.

1. **The banner** says what people can't do right now and what still works.
   Its details hold the reason as the part gave it.
2. **The part that fails** is marked on its edge. Its state says so in words,
   and the sentence beside it says since when, and when it last worked.
3. **The check** shows what the failure costs: the kind of machine that can't
   be signed in on, and the step that failed.

The banner speaks of the people, not of the part. The other parts follow the
same form:

| Part that fails | The banner says |
|---|---|
| the certificate authority | Nobody can be signed in on a Windows machine as themselves right now. Kept accounts, and typing a name and a password, still work. |
| the sign-in service | Nobody can sign in to Away right now. |
| the records | No machine can be opened right now, because Away can't keep its records. |

- The banner stays for as long as the part fails, and can't be closed. When
  the part answers again, the banner goes and the row says well.
- More than one failing part gives one banner each, in the order of the table.

## 15.4 States

- **Loading.** The rows show as grey blocks until each part has answered. A
  part that takes longer than the others keeps its block, so the rest is not
  held up.
- **Couldn't load.** The tab says so in its place
  ([chapter 16](./errors.md)).
- **The check is running.** Its button shows a loader and can't be pressed
  again; the line beside it says “Checking now.”

## 15.5 Words

| Where | Text |
|---|---|
| The tab | status |
| Over the first table | away's parts · Asked at 09:14, and again every minute. |
| The first table's columns | part · state · what it last did |
| The parts | the sign-in service · signing in on machines · the certificate authority · the records · the files |
| Under a part's name | Where people sign in to Away, at sso.example.com. · The part that makes the sign-ins on machines, kept apart from the rest. · Issues what signs a person in on a Windows machine in a domain. · Every connection, sign-in and change. · The machines, groups and rules in force. |
| A state | well · not answering |
| What a part last did | Answered in 84 ms. · Issued a certificate for a test at 09:10. · Last written at 09:14, and put out for the log system at 09:14. · Applied on 13 nov 2026 at 09:02. None refused. |
| A part that fails | No answer since 09:02. Its last certificate was issued at 08:58. |
| Over the second table | the check of the setup · Last run today at 09:11, by you. · check again · Checking now. |
| The second table's columns | kind of machine · tried on · what was tried · how it went |
| The kinds of machine | a Windows machine in a domain · a shell · a desktop with a kept account |
| What was tried | Signing in as you, with nothing typed. · Signing in with the account Away keeps for it. |
| How it went | worked · failed, with the reason under it: the authority gave no certificate |
| The foot | Away 0.1.0 · When something fails, people are told to turn to it-help@example.com, as the configuration says. · Nobody is named for people to turn to. |
| The banners | the texts of the table in 15.3 |

## 15.6 What the figures draw

<!-- generated: draws -->
| Figure | Draws |
|---|---|
| 15.1 Status, with every part well | [See whether Away and what it rests on are well](../capabilities/platform.md#running-it), [Check the whole setup and say what is wrong: the provider, the authority, each kind of machine](../capabilities/platform.md#installing-and-configuring), [Keep machines, groups and rules as files, applied as they change](../capabilities/admin.md#the-register-of-machines), [See that a changed file was refused, and why, while what was in force stays](../capabilities/admin.md#the-register-of-machines), [Send every record on to the organisation's own log system as it is written](../capabilities/admin.md#what-happened), [Run the part that signs people in on machines apart from the rest, with rights of its own](../capabilities/platform.md#installing-and-configuring), [Say whom people turn to, and show it wherever something fails](../capabilities/platform.md#installing-and-configuring), [See in Away itself which version runs, where its source is and the licences of what it is built from](../capabilities/platform.md#checking-it) |
| 15.2 Status, while the certificate authority doesn't answer | [See whether Away and what it rests on are well](../capabilities/platform.md#running-it), [Check the whole setup and say what is wrong: the provider, the authority, each kind of machine](../capabilities/platform.md#installing-and-configuring) |
<!-- end: draws -->
