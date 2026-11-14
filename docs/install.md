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

It needs no Windows domain, no certificate authority and no directory.

## Installing it

On Kubernetes, with an ingress:

```sh
helm install away oci://ghcr.io/nalet/charts/away --version <version> \
  --namespace away --create-namespace \
  --set address=away.example.com \
  --set ingress.enabled=true
```

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
          kubernetes.io/metadata.name: ingress-nginx
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
