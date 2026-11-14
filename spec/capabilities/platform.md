# The platform

Away as it is installed, run and checked.
[How to read the tables](./README.md#how-to-read-it).

<!-- generated: areas -->
## Installing and configuring

Away as an operator installs it: where it runs, the identity provider, addresses, certificates, what signing in on machines needs. For the goal [Run it oneself](../goals/README.md#run-it-oneself).

| Capability | Who | Away | Key |
|---|---|---|---|
| Install it on Kubernetes or OpenShift with one chart | operator | v1 | `install-with-chart` |
| Run with no special rights on the cluster | system | v1 | `run-without-special-rights` |
| Run it as one container on one machine | operator | later | `install-as-one-container` |
| Use the identity provider that is there, over OpenID Connect | operator | v1 | `use-own-provider` |
| Say which part of a person's sign-in names their account on the machines | operator | v1 | `name-account-claim` |
| Bring an identity provider along for those who have none | operator | no | `bring-provider-along` |
| Set the address it answers on, behind the reverse proxy that is there | operator | v1 | `set-address` |
| Use the cluster's certificates, or one's own | operator | v1 | `use-own-certificates` |
| Choose who issues the sign-ins on a Windows domain's machines: Away's own authority or the organisation's | operator | v1 | `connect-domain-authority` |
| Give a domain's controllers the certificates they need, where the domain has no authority of its own | operator | v1 | `certify-domain-controllers` |
| Have the shells trust Away's sign-in, with one line on each machine | operator | v1 | `make-shells-trust` |
| Run the part that signs people in on machines apart from the rest, with rights of its own | operator | v1 | `sign-apart` |
| Keep what has to stay secret encrypted, with the key kept elsewhere | system | v1 | `protect-secrets` |
| Check the whole setup and say what is wrong: the provider, the authority, each kind of machine | operator | v1 | `check-setup` |
| Say whom people turn to, and show it wherever something fails | operator | v1 | `name-support-contact` |
| Reach machines in a network Away is not in, through a relay placed there | operator | later | `reach-other-networks` |
| Carry other traffic to the machines' network, as a private network would | operator | no | `carry-other-traffic` |

- *Say which part of a person's sign-in names their account on the machines*: a Windows domain knows a person by a name and an identifier of its own, a shell by another name.
- *Bring an identity provider along for those who have none*: Away asks an identity provider and is none.
- *Choose who issues the sign-ins on a Windows domain's machines: Away's own authority or the organisation's*: Away's own is made known to the domain once; the organisation's issues on Away's request; an installation without a domain sets neither.
- *Give a domain's controllers the certificates they need, where the domain has no authority of its own*: signing in with a certificate needs them, and Away's own authority issues them on the operator's request.
- *Carry other traffic to the machines' network, as a private network would*: Away carries sessions and no other traffic.

## Running it

Keeping it running: updates, health, measures, backups, more than one instance. For the goal [Run it oneself](../goals/README.md#run-it-oneself).

| Capability | Who | Away | Key |
|---|---|---|---|
| Update to a new version in one step, with people told and their desktops back by themselves | operator | v1 | `update-in-one-step` |
| Go back to the version before | operator | later | `go-back-a-version` |
| See whether Away and what it rests on are well | operator | v1 | `see-health` |
| Give the monitoring that is there its measures: sessions, sign-ins, failures, delay | system | v1 | `give-measures` |
| Write what happens as lines a log collector reads | system | v1 | `write-logs` |
| Back up what Away holds and bring it back elsewhere | operator | later | `back-up` |
| Run more than one instance, so that load is shared and one may fail | operator | later | `run-several` |
| Do by a script everything an administrator does in the browser | operator | later | `do-by-script` |
| Collect what a helper needs to see into one file, with every secret left out | operator | later | `collect-for-support` |

## Checking it

What anyone can do to see that Away is what it says. For the goal [Trust it](../goals/README.md#trust-it).

| Capability | Who | Away | Key |
|---|---|---|---|
| Check that an image is the one the project built | anyone | v1 | `verify-image` |
| See what an image holds: every part, its version, its licence | anyone | v1 | `see-image-contents` |
| Build it from its source with one command | anyone | v1 | `build-from-source` |
| Run its tests on one's own machine, against a real server of each protocol | anyone | v1 | `run-its-tests` |
| Run it with no call to anyone outside: no fonts, scripts, statistics or update checks | system | v1 | `call-nobody-else` |
| See in Away itself which version runs, where its source is and the licences of what it is built from | anyone | v1 | `see-version-and-licences` |
| Report a security hole in private, and learn what happens next | anyone | v1 | `report-in-private` |
<!-- end: areas -->
