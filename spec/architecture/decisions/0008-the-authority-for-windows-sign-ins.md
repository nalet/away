# ADR-0008: Who issues Windows sign-ins is a setting, and without a domain nobody has to

**Status:** Proposed · 2026-11-14. Revised on the owner's word: who issues has to be a setting, and Away has to work where there is no Active Directory. Nothing of it is built. Each way is proven on a real domain before it is built on (see [Validation](#validation)).

## Context

[ADR-0007](./0007-short-lived-certificates.md) signs a person in on a Windows machine with a certificate. A domain accepts one for signing in when:

- the authority that issued it is on the domain's list of authorities trusted for sign-ins;
- the certificate names the account, by the name people sign in with and by the identifier the directory gave it, which domain controllers have insisted on since 2025;
- the domain controllers can fetch the authority's list of certificates it withdrew;
- the domain controllers have certificates of their own, from an authority on the same list.

Organisations differ in what they have:

| An organisation has | So |
|---|---|
| no Windows domain at all | there is nobody to issue for, and nothing of this may be needed |
| a domain, and no certificate authority | somebody has to issue: for people, and for the domain controllers too |
| a domain with a certificate authority of its own | it can issue itself, and hold the limits itself |

Two authorities could issue:

| Authority | What the domain has to do | Who says no to an administrator's account |
|---|---|---|
| Away's own | put Away's authority on its list, once | Away alone: the issuer's own list |
| the organisation's own | allow Away to ask for certificates in other people's names, for one kind of certificate and one group of people | the directory itself |

With Away's own authority, the key in the issuer can make a certificate for any account, and only the issuer's checks stand in the way. With the organisation's, that key is worth what the directory allows it. Which of the two is right depends on the organisation, so it is not decided here once for everyone.

## Decision

1. **Signing in as oneself on Windows machines is a part of Away that an installation turns on.** It is on when the configuration names a domain. Without one, Away has no authority for Windows sign-ins, asks for none, reaches no domain controller, and the status page lists none.
2. **Nothing else in Away needs a Windows domain, or a directory of any kind.** People and their groups come from the sign-in service, whatever stands behind it. On a shell a person is themselves by the issuer's own authority for shells, which needs no directory. A desktop is opened with a kept account or with what the person types, a Windows desktop too.

   | Machine | As oneself | Otherwise |
   |---|---|---|
   | a Windows machine in the domain the configuration names | by a certificate | a kept account, or typing |
   | a Windows machine in no domain | no: it knows accounts of its own alone | a kept account, or typing |
   | a Windows machine joined to a cloud directory only | later (`be-oneself-cloud-joined`) | a kept account, or typing |
   | a shell | by a certificate of the issuer's own authority for shells | a kept account, or typing |
   | a desktop over VNC | no: VNC knows no identity | a kept account, or typing |

3. **Who issues is a setting.** For the domain it names, the configuration says one of two:
   - **Away's own authority.** The issuer signs the certificates itself, with a key it makes at installation or one the operator brings.
   - **The organisation's own authority.** The issuer asks that authority for each certificate, in the person's name.

   Both sit behind one seam in the issuer: the rest of Away asks for a certificate and doesn't know who signs it. Changing the setting changes nothing else.
4. **What else is set, in the same place:** the domain's name and, where they aren't found by themselves, its domain controllers; how long a certificate lasts, five minutes at most; the accounts and groups nobody is signed in as; which entries of a person's sign-in name the account and its identifier (`name-account-claim`). Each machine's file says whether people are signed in there as themselves, with a kept account, or by typing (`set-sign-in-way`).
5. **Where a domain has no authority of its own, Away's serves the whole of it.** It also issues the certificates the domain controllers need, from a request the operator makes on each one, with a command of Away's (`certify-domain-controllers`). The issuer hands the server its list of withdrawn certificates, which the server publishes at Away's address for the domain controllers to fetch.
6. **Where the organisation has an authority, the guide recommends it**, because the directory then holds the limit. Away's own stays possible there.
7. **The certificate names the account by what the person's sign-in says**: its name and its identifier in the directory. Where the organisation's authority issues, it writes them itself from the directory.
8. **Nothing of Away's runs on a Windows machine**: no service beside the authority, no helper on a domain controller.
9. **The check of the setup signs a test account in** on a machine of the domain and says, when it fails, which of these it was: the authority is unknown to the domain, the account's name didn't match, the list of withdrawn certificates couldn't be fetched, a domain controller has no certificate, the organisation's authority refused ([chapter 15](../../screens/admin-status.md)). An installation without a domain has no such row.

### Validation

- **Without a domain.** Every test that needs no domain runs on an installation that names none, from the first work package on.
- **Away's own authority**, with the domain controllers' certificates from it too, is proven in [package 3](../../plan/README.md), on a domain that exists for testing and has no authority of its own. A domain in use should not be asked to trust a test.
- **The organisation's own authority** is proven at the start of [package 12](../../plan/README.md): Away asks an authority for a certificate in another person's name over one of its published interfaces, with nothing of Away's installed there. Which interface is not settled. If none serves, the owner chooses between a small helper beside the authority, which point 8 rules out today, and offering Away's own authority alone.

## Consequences

- An organisation without a Windows domain installs Away and lacks nothing but what a domain would give.
- An organisation with a domain and no authority can still sign people in as themselves, at the price of trusting Away's authority with the whole domain.
- An organisation with an authority keeps the limit in its own directory, once package 12 is built.
- With Away's own authority, its key is the most valuable thing Away holds: whoever has it is anyone in the domain for as long as nobody notices. It lives in the issuer alone ([ADR-0003](./0003-three-parts-with-their-own-rights.md), [ADR-0010](./0010-secrets-live-in-the-cluster.md)), and the guide says how to replace it.
- Where Away's authority also certifies the domain controllers, those certificates run out and are made again by hand. Signing in as oneself stops when one is forgotten, and the check of the setup says that this is why.
- Putting an authority on a domain's list is done by whoever answers for the domain, once, by hand. Away can check it and can't do it.
- Two ways are two things to build, to test and to explain.

### What this rules out

- **No Windows domain that Away needs in order to run.**
- **No helper of Away's on a Windows machine.**
- **No sign-in as oneself on a Windows machine outside a domain** in the first release. Such a machine takes a kept account or what the person types.
- **No certificate for an account the person's sign-in doesn't name.**
