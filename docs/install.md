# Installing Away

Away is installed on Kubernetes or OpenShift with one chart, by someone who may
act in one namespace. It asks for no rights over the cluster.

> **What this version is.** Away's three parts run and say whether they are
> well. Someone who opens Away is signed in through the organisation's sign-in
> service and sees the frame of Away's pages: home, the sessions, the settings,
> and signing out. There is no machine to open yet.
> [The work packages](../spec/plan/README.md) say what comes next.

## What it needs

| Needed | For |
|---|---|
| a namespace, and the right to make workloads in it | everything |
| helm 3.8 or newer | installing the chart |
| an address, such as `away.example.com`, that leads to the cluster's ingress controller or router | reaching Away from outside the cluster |
| a cluster that enforces network rules | the cluster refusing what Away doesn't need; without it Away runs, and nothing is refused |
| a sign-in service that speaks OpenID Connect, and a client of it for Away | signing in; without it Away runs, and nobody can sign in |
| a PostgreSQL database, and an account of it for Away | keeping what people star; without it Away runs, and keeps nothing |

It needs no Windows domain, no certificate authority and no directory.

## Installing it

On Kubernetes, with an ingress:

```sh
helm install away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --create-namespace \
  --set address=away.example.com \
  --set ingress.enabled=true
```

With a certificate of one's own for the address, keep it in the namespace as
a secret of the kind `kubernetes.io/tls` and name it:

```sh
kubectl --namespace away create secret tls away-address --cert=address.pem --key=address.key
helm upgrade away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --reuse-values \
  --set ingress.tlsSecret=away-address
```

Away is reached over https alone. An ingress controller decides by itself what
it does with a request that arrives without TLS: set it to send such a request
on to https.

On OpenShift, with a route:

```sh
helm install away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --create-namespace \
  --set address=away.example.com \
  --set route.enabled=true
```

Until the first release, the versions are those the main branch publishes, such
as `0.1.0-main.34d0117`. [Building and checking](./build.md#checking-a-published-image)
says how to check an image before installing it.

## Signing in

People sign in where the organisation already signs in. Away is a client of
that sign-in service, over OpenID Connect, and keeps no people of its own.

At the sign-in service, make a client for Away:

| Asked there | Give |
|---|---|
| the kind of client | one with a secret, that signs people in with a code |
| where people are sent back to | `https://away.example.com/sign-in/back`, and nothing else |
| the proof that whoever began a sign-in ends it (PKCE) | S256; require it where the service can |
| renewals (refresh tokens) | on, so that a sign-in renews itself. Some services give them only when `offline_access` is asked for, which then goes into `signIn.asksFor` |
| groups | an entry in what the service signs that names a person's groups. Its name goes into `signIn.groupsEntry` |

Put the client's secret into a secret of the cluster, from a file, so that it is
in no history and in no file of values:

```sh
kubectl --namespace away create secret generic away-sign-in --from-file=secret=./the-clients-secret
```

Then tell the chart:

```yaml
address: away.example.com
signIn:
  service: https://sign-in.example.com/realms/staff
  client: away
  secretName: away-sign-in
administrators: [away-admins]
recordReaders: [auditors]
contact: it-help@example.com
```

- `signIn.service` is the service as it names itself: what it says under
  `issuer` at `/.well-known/openid-configuration`.
- For a service whose certificate is of the organisation's own authority, put
  that authority's certificate into a ConfigMap, as PEM under the key
  `authorities.pem`, and name the ConfigMap in `signIn.authoritiesConfigMap`.
- The server holds each sign-in and gives the browser a cookie that names it
  and says nothing else. No token reaches the page.
- A sign-in is renewed a minute before what the service signed runs out. Who
  the person is and which groups they are in is taken anew each time, so
  someone taken out of a group is out of it in Away with the next renewal.
- This version holds the sign-ins in memory: when the server starts again,
  people sign in again.
- With the network rules on, the server reaches the sign-in service's port,
  443 unless `networkRules.signInServicePort` says otherwise, and the
  cluster's name service. `networkRules.signInServiceAt` narrows that to the
  addresses it names.

## The machines and the rules

Away knows machines, their groups and the rules from files, which the chart
takes as values or from a ConfigMap of one's own.
[The machines and the rules](./machines.md) says how they are written and how a
change is applied.

## The database

Away keeps what people star in one PostgreSQL database, which only the server
reaches. Without one Away runs, keeps nothing, and the pages show no stars.

Bring a database and an account for Away. The account makes Away's tables
itself, so it owns the database or may create in it:

```sql
create role away login password '…';
create database away owner away;
```

```sh
kubectl --namespace away create secret generic away-database --from-file=password=./password
helm upgrade away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --reuse-values \
  --set database.host=postgres.example.com --set database.secretName=away-database
```

- **The connection is secured**, and the database has to show a certificate
  for the name in `database.host`. Where the certificate is of the
  organisation's own authority, name it: a ConfigMap with its certificate as
  PEM, under the key `authorities.pem`, in `database.authoritiesConfigMap`.
- **A database that shows a certificate no authority signed** is reached with
  `database.connection=encrypted`: the connection is encrypted, and the
  certificate is not checked. What travels can't be read on the way, and
  whoever can put themselves between the server and the database is not
  noticed, so it is for a network nobody else is on. `database.connection=plain`
  secures nothing.
- **The server signs in the way the database asks.** A PostgreSQL of today
  asks for a proof of the password (SCRAM-SHA-256), and the password itself
  never travels. Let the database ask for that: over a connection that is
  not secured, a database set to ask for the password itself is given it.
- **The server brings the database into shape itself** when it starts, and
  writes a line: `the database was brought into shape`. A later version takes
  the steps it adds, and never one back: a version that finds the database in
  the shape of a later one says so and keeps nothing.
- **A database that can't be reached keeps nobody from the machines.** The
  pages show no stars for as long, the server writes why, once, and tries
  again.
- With the network rules on, the server reaches the database's port and the
  cluster's name service. `networkRules.databaseAt` narrows that to the
  addresses it names.

Away is tried against PostgreSQL 17 on every change.

## The farms' feeds

A farm of Windows machines that the files name says itself what it publishes
([a farm](./machines.md#a-farm)). Away asks its feed as an account of the
organisation's directory, which the installation gives it:

```sh
kubectl --namespace away create secret generic away-farms --from-file=password=./password
helm upgrade away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --reuse-values \
  --set 'farms.account=CORP\svc-away' --set farms.secretName=away-farms
```

- **An account for this alone.** It is in the groups the farms publish to, so
  that a farm shows it what there is, and it needs nothing else. Where the
  directory can, refuse it signing in on the machines themselves: Away never
  does that with it.
- **Its password is a secret of the cluster**, under the key `password`. The
  server reads it as it starts, so a changed password takes a restart of the
  server.
- **A web access with a certificate of the organisation's own authority**
  needs that authority named: a ConfigMap with its certificate as PEM, under
  the key `authorities.pem`, in `farms.authoritiesConfigMap`.
- With the network rules on, the server reaches the farms' port, 443 unless
  `networkRules.farmsPort` says otherwise, and the cluster's name service.
  `networkRules.farmsAt` narrows that to the addresses it names.

## What it makes

| What | For |
|---|---|
| three workloads from the one image: the server, the gateway, the issuer | Away's parts, each with an account that may ask the cluster for nothing |
| an ingress, or two routes | the way in: on Away's address, a session's connection goes to the gateway and everything else to the server |
| network rules | nothing reaches a part and a part reaches nothing, except the connections [the architecture lists](../spec/architecture/connections.md) |
| a ServiceMonitor and alert rules, if asked | for a cluster that runs the Prometheus operator |

Every container runs as an ordinary user and never as root, can't write to its
own files and has no capabilities. A namespace that takes only the strictest
profile takes Away as it is.

## The values

Everything an installation is told is one of these. The chart's
[values.yaml](../chart/values.yaml) has each with its meaning.

| Value | Means | Default |
|---|---|---|
| `address` | the address Away answers on | none; a way in and signing in need it |
| `signIn.service` | the sign-in service, as it names itself | none; without it nobody signs in |
| `signIn.client` | the name the sign-in service knows Away by | none |
| `signIn.secretName` | the secret that holds the client's secret, under the key `secret` | none |
| `signIn.authoritiesConfigMap` | a ConfigMap with certificates of authorities to trust besides the public ones, under the key `authorities.pem` | none |
| `signIn.asksFor` | what the service is asked to say of a person | `openid profile email` |
| `signIn.groupsEntry` | the entry of a sign-in that names the person's groups | `groups` |
| `administrators` | the groups whose people administer Away | none |
| `recordReaders` | the groups whose people read its records | none |
| `contact` | whom people turn to when something fails | none |
| `files` | the machines, their groups and the rules, as files: each entry a file's name and what it holds ([the machines and the rules](./machines.md)) | none: Away knows no machines |
| `pictures` | the pictures the files name as icons: each entry a picture's name and the picture in Base64 | none |
| `filesConfigMap` | a ConfigMap of one's own that holds those files and pictures, in place of `files` and `pictures` | none |
| `database.host`, `database.port` | the database server's name on the network, and its port ([the database](#the-database)) | none: Away keeps nothing; port 5432 |
| `database.name`, `database.account` | the database of that server which is Away's, and the account the server signs in as | `away`, `away` |
| `database.secretName` | the secret that holds that account's password, under the key `password` | none |
| `database.connection` | how the connection to the database is secured: `verified`, `encrypted` without checking the database's certificate, or `plain` | `verified` |
| `database.authoritiesConfigMap` | a ConfigMap with certificates of authorities to trust for the database besides the public ones, under the key `authorities.pem` | none |
| `farms.account` | the account Away reads the farms' feeds with, such as `CORP\svc-away` ([the farms' feeds](#the-farms-feeds)) | none: a farm in the files shows nothing |
| `farms.secretName` | the secret that holds the account's password, under the key `password` | none |
| `farms.authoritiesConfigMap` | a ConfigMap with certificates of authorities to trust for the farms' web access besides the public ones, under the key `authorities.pem` | none |
| `image.repository`, `image.tag`, `image.digest` | the image; a digest goes before a tag | the project's image, in the chart's version |
| `image.pullSecrets` | secrets to pull the image with | none |
| `ingress.enabled` | make an ingress | no |
| `ingress.className` | the ingress controller's class | the cluster's default |
| `ingress.tlsSecret` | the secret that holds the certificate for the address | the ingress controller's own certificate |
| `route.enabled` | make routes, on OpenShift | no |
| `route.certificate`, `route.key`, `route.caCertificate` | one's own certificate in place of the cluster's; give them with `--set-file` | the cluster's certificate |
| `route.timeout` | how long the router leaves a session's connection open while nothing passes | `1h` |
| `networkRules.enabled` | make the network rules | yes |
| `networkRules.wayInFrom` | who may reach Away's address, as the peers of a NetworkPolicy | the cluster's router with a route; every namespace otherwise |
| `networkRules.measuresFrom` | who may read the measures | nobody |
| `networkRules.signInServiceAt`, `networkRules.signInServicePort` | where the sign-in service is, as the peers of a NetworkPolicy, and its port | any address, port 443 |
| `networkRules.databaseAt` | where the database is, as the peers of a NetworkPolicy; used once `database.host` is set | any address, at `database.port` |
| `networkRules.farmsAt`, `networkRules.farmsPort` | where the farms' web access is, as the peers of a NetworkPolicy, and its port; used once `farms.account` is set | any address, port 443 |
| `monitoring.serviceMonitor`, `monitoring.alerts` | make a ServiceMonitor, and alert rules | no |
| `server.resources`, `gateway.resources`, `issuer.resources` | what each part may use | small requests, and a limit on memory |
| `nodeSelector`, `tolerations` | where the parts run | anywhere |

With an ingress controller that runs in a namespace of its own, name it in
`networkRules.wayInFrom`, so that nothing else in the cluster reaches Away's
address:

```yaml
networkRules:
  wayInFrom:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: ingress
```

## Seeing that it runs

```sh
kubectl --namespace away get pods
kubectl --namespace away logs deployment/away-server
```

Each part writes one line an event, as an object with its time, its level, the
part and what happened. Each also answers `/livez`, `/readyz` and `/metrics` on
port 9090, which the way in never leads to
([what a part answers](./build.md#what-a-part-answers)).

For a monitoring that the Prometheus operator runs, let its namespace read the
measures and ask for the two objects:

```yaml
networkRules:
  measuresFrom:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: monitoring
monitoring:
  serviceMonitor: true
  alerts: true
```

## Updating and removing it

```sh
helm upgrade away oci://ghcr.io/nalet/charts/away --version <new version> --namespace away --reuse-values
helm uninstall away --namespace away
```
