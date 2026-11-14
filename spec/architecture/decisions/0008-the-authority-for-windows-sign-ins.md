# ADR-0008: Who issues the certificates a Windows domain accepts

**Status:** Proposed · 2026-11-14. Nothing of it is built. Each of its two ways is proven on a real domain before it is built on (see [Validation](#validation)).

## Context

[ADR-0007](./0007-short-lived-certificates.md) signs a person in on a Windows machine with a certificate. A domain accepts one for signing in when:

- the authority that issued it is on the domain's list of authorities trusted for sign-ins;
- the certificate names the account, by the name people sign in with and by the identifier the directory gave it, which domain controllers have insisted on since 2025;
- the domain controllers can fetch the authority's list of certificates it withdrew;
- the domain controllers have certificates of their own.

The capability `connect-domain-authority` names two authorities that could issue:

| Authority | What the domain has to do | Who says no to an administrator's account |
|---|---|---|
| Away's own | put Away's authority on its list, once | Away alone: the issuer's own list |
| the organisation's own | allow Away to ask for certificates in other people's names, for one kind of certificate and one group of people | the directory itself |

With Away's own authority, the key in the issuer can make a certificate for any account, and only the issuer's checks stand in the way. With the organisation's, that key is worth what the directory allows it: ordinary accounts of one group, and nothing else.

## Decision

1. **Both ways sit behind one seam in the issuer.** The rest of Away asks for a certificate and doesn't know who signs it.
2. **Away's own authority is built first.** It needs nothing on the Windows side but trust, works where an organisation has no authority, and is the way every test can run. The issuer makes its key at installation or takes one the operator brings; the issuer hands the server its list of withdrawn certificates, which the server publishes at Away's address; the installation guide gives the commands that make the authority known to a domain.
3. **The organisation's own authority follows**, as a package of its own, for an organisation that has one. It is the way to recommend there, because the directory then holds the limit.
4. **The certificate names the account by what the person's sign-in says**: its name and its identifier in the directory, as the operator named them (`name-account-claim`). Where the organisation's authority issues, it writes them itself from the directory.
5. **Nothing of Away's runs on a Windows machine**: no service beside the authority, no helper on a domain controller.
6. **The check of the setup signs a test account in** on a machine of the domain and says, when it fails, whether the authority is unknown to the domain, the account's name didn't match, or the list of withdrawn certificates couldn't be fetched ([chapter 15](../../screens/admin-status.md)).

### Validation

- **Away's own authority** is proven in [package 3](../../plan/README.md), on a domain that exists for testing and trusts a test authority. A domain in use should not be asked to trust a test.
- **The organisation's own authority** is proven at the start of [package 12](../../plan/README.md): Away asks an authority for a certificate in another person's name over one of its published interfaces, with nothing of Away's installed there. Which interface is not settled. If none serves without a helper on the Windows side, the package ends there and the first release ships with Away's own authority alone.

## Consequences

- An organisation without an authority of its own can still sign people in as themselves.
- An organisation with one keeps the limit in its own directory, once package 12 is built.
- With Away's own authority, its key is the most valuable thing Away holds: whoever has it is anyone in the domain for as long as nobody notices. It lives in the issuer alone ([ADR-0003](./0003-three-parts-with-their-own-rights.md), [ADR-0010](./0010-secrets-live-in-the-cluster.md)), and the guide says how to replace it.
- Putting an authority on a domain's list is done by whoever answers for the domain, once, by hand. Away can check it and can't do it.
- A domain controller has to reach Away's address for the list of withdrawn certificates, where Away's own authority issues.

### What this rules out

- **No helper of Away's on a Windows machine.**
- **No sign-in as oneself on a domain that trusts neither authority.** Such machines take a kept account or what the person types.
- **No certificate for an account the person's sign-in doesn't name.**
