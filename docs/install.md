# Installing Away

Away is installed on Kubernetes or OpenShift with one chart, by someone who may
act in one namespace. It asks for no rights over the cluster.

> **What this version is.** The first work package: Away's three parts run,
> say whether they are well and answer on Away's address with one page. There
> is nothing to sign in to and no machine to open yet.
> [The work packages](../spec/plan/README.md) say what comes next.

## What it needs

| Needed | For |
|---|---|
| a namespace, and the right to make workloads in it | everything |
| helm 3.8 or newer | installing the chart |
| an address, such as `away.example.com`, that leads to the cluster's ingress controller or router | reaching Away from outside the cluster |
| a cluster that enforces network rules | the cluster refusing what Away doesn't need; without it Away runs, and nothing is refused |

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
| `address` | the address Away answers on | none; a way in needs it |
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
