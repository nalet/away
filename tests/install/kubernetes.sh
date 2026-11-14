#!/usr/bin/env bash
# Installs Away on the cluster that kubectl points at, as someone with rights in one
# namespace and nowhere else, and checks what the first work package promises of an
# installation:
#
#   - one command installs it, into a namespace that takes only the strictest profile;
#   - its three parts say that they are well, give their measures, and write their lines;
#   - the server answers on its address and the gateway on its own;
#   - the cluster refuses every connection the architecture doesn't list.
#
#   tests/install/kubernetes.sh
#
# It needs kubectl, helm, curl and jq, a cluster that enforces network rules, and the
# image in a place the cluster can pull it from (or already has):
#
#   IMAGE_REPOSITORY   the image, without its tag           (ghcr.io/nalet/away)
#   IMAGE_TAG          its tag                              (main)
#   IMAGE_PULL_POLICY  IfNotPresent, or Never for an image the cluster already holds
#   NAMESPACE          where it is installed, and removed again after   (away-test)
#   KEEP               set to 1 to leave the installation in place
set -euo pipefail

IMAGE_REPOSITORY=${IMAGE_REPOSITORY:-ghcr.io/nalet/away}
IMAGE_TAG=${IMAGE_TAG:-main}
IMAGE_PULL_POLICY=${IMAGE_PULL_POLICY:-IfNotPresent}
NAMESPACE=${NAMESPACE:-away-test}
OUTSIDE=${NAMESPACE}-outside
# a small image with curl in it, for the pods that try the connections
PROBE_IMAGE=curlimages/curl:8.23.0@sha256:3fb7e13e5d179a845c97d78204f3f1064d1907037f26a3ebf91710571151fd93
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
WORK=$(mktemp -d)
FORWARDS=()

say() { printf '\n== %s\n' "$*"; }
ok() { printf '   ok   %s\n' "$*"; }
fail() { printf '   FAIL %s\n' "$*" >&2; exit 1; }

clean() {
  for pid in "${FORWARDS[@]:-}"; do [ -n "$pid" ] && kill "$pid" 2>/dev/null || true; done
  if [ "${KEEP:-}" != 1 ]; then
    kubectl delete namespace "$NAMESPACE" "$OUTSIDE" --ignore-not-found --wait=false >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK"
}
trap clean EXIT

# forward <pod> <port>: a way to one port of one pod, on a port of this machine
forward() {
  local log="$WORK/forward-$1-$2.log"
  kubectl --namespace "$NAMESPACE" port-forward "pod/$1" ":$2" >"$log" 2>&1 &
  FORWARDS+=($!)
  for _ in $(seq 1 50); do
    if grep -q 'Forwarding from 127.0.0.1:' "$log"; then
      sed -n 's/^Forwarding from 127\.0\.0\.1:\([0-9]*\) .*/\1/p' "$log" | head -1
      return 0
    fi
    sleep 0.2
  done
  fail "no way to port $2 of $1: $(cat "$log")"
}

# probe <namespace> <name> [labels]: a pod that waits, to try connections from
probe() {
  local labels=${3:-}
  kubectl --namespace "$1" apply -f - >/dev/null <<YAML
apiVersion: v1
kind: Pod
metadata:
  name: $2
  labels: {$labels}
spec:
  automountServiceAccountToken: false
  securityContext:
    runAsNonRoot: true
    runAsUser: 65532
    seccompProfile: {type: RuntimeDefault}
  containers:
    - name: probe
      image: $PROBE_IMAGE
      command: [sleep, "3600"]
      securityContext:
        allowPrivilegeEscalation: false
        readOnlyRootFilesystem: true
        capabilities: {drop: [ALL]}
YAML
  kubectl --namespace "$1" wait --for=condition=Ready "pod/$2" --timeout=120s >/dev/null
}

# reaches <namespace> <pod> <address>: whether a connection from the pod is taken
reaches() {
  kubectl --namespace "$1" exec "$2" -- curl --silent --output /dev/null --max-time 4 "http://$3" >/dev/null 2>&1
}

say "A namespace that takes only the strictest profile, and someone who may act in it alone"
kubectl create namespace "$NAMESPACE" >/dev/null
kubectl label namespace "$NAMESPACE" pod-security.kubernetes.io/enforce=restricted pod-security.kubernetes.io/enforce-version=latest >/dev/null
kubectl create namespace "$OUTSIDE" >/dev/null
kubectl --namespace "$NAMESPACE" create serviceaccount installer >/dev/null
kubectl --namespace "$NAMESPACE" create rolebinding installer --clusterrole=admin --serviceaccount="$NAMESPACE:installer" >/dev/null
TOKEN=$(kubectl --namespace "$NAMESPACE" create token installer --duration=30m)
kubectl config view --minify --raw --output json \
  | jq --arg token "$TOKEN" --arg namespace "$NAMESPACE" \
       '.users = [{name: "installer", user: {token: $token}}] | .contexts[0].context.user = "installer" | .contexts[0].context.namespace = $namespace' \
  > "$WORK/installer.kubeconfig"
if KUBECONFIG="$WORK/installer.kubeconfig" kubectl auth can-i create customresourcedefinitions --all-namespaces >/dev/null 2>&1; then
  fail "the installer has rights over the whole cluster, so this proves nothing"
fi
ok "the installer may act in $NAMESPACE and nowhere else"

say "One command installs it"
KUBECONFIG="$WORK/installer.kubeconfig" helm install away "$ROOT/chart" --namespace "$NAMESPACE" \
  --set image.repository="$IMAGE_REPOSITORY" --set image.tag="$IMAGE_TAG" --set image.pullPolicy="$IMAGE_PULL_POLICY" \
  --wait --timeout 240s >/dev/null || {
    kubectl --namespace "$NAMESPACE" get pods,events >&2 || true
    fail "helm install"
  }
ok "helm install, with its three parts ready"

say "Each part says that it is well, gives its measures and writes its lines"
for part in server gateway issuer; do
  pod=$(kubectl --namespace "$NAMESPACE" get pods --selector "app.kubernetes.io/instance=away,app.kubernetes.io/component=$part" --output jsonpath='{.items[0].metadata.name}')
  port=$(forward "$pod" 9090)
  [ "$(curl --silent "http://127.0.0.1:$port/livez")" = alive ] || fail "$part: /livez"
  [ "$(curl --silent "http://127.0.0.1:$port/readyz")" = ready ] || fail "$part: /readyz"
  curl --silent "http://127.0.0.1:$port/metrics" > "$WORK/$part.measures"
  grep -q "^away_ready{part=\"$part\"} 1\$" "$WORK/$part.measures" || fail "$part: its measures don't say it is ready"
  grep -q "^away_build_info{part=\"$part\",version=" "$WORK/$part.measures" || fail "$part: its measures don't name its version"
  kubectl --namespace "$NAMESPACE" logs "$pod" > "$WORK/$part.lines"
  [ -s "$WORK/$part.lines" ] || fail "$part: wrote no line"
  jq --exit-status --slurp --arg part "$part" 'all(.[]; .time and .level and .event and .part == $part)' "$WORK/$part.lines" >/dev/null \
    || fail "$part: a line is not an object with its time, its level, its part and what happened"
  user=$(kubectl --namespace "$NAMESPACE" get pod "$pod" --output jsonpath='{.spec.containers[0].securityContext.readOnlyRootFilesystem}/{.spec.automountServiceAccountToken}')
  [ "$user" = true/false ] || fail "$part: it can write its own files, or holds a token for the cluster ($user)"
  ok "$part: alive, ready, $(grep -c '^away_' "$WORK/$part.measures") measures, $(wc -l < "$WORK/$part.lines" | tr -d ' ') lines"
done

say "The server answers on its address, and the gateway on its own"
server=$(kubectl --namespace "$NAMESPACE" get pods --selector app.kubernetes.io/component=server --output jsonpath='{.items[0].metadata.name}')
gateway=$(kubectl --namespace "$NAMESPACE" get pods --selector app.kubernetes.io/component=gateway --output jsonpath='{.items[0].metadata.name}')
port=$(forward "$server" 8080)
curl --silent "http://127.0.0.1:$port/" | grep -q 'Away is installed and running.' || fail "the server's page"
[ "$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$port/livez")" = 404 ] || fail "the server says whether it is well on the address Away is reached at"
ok "the server's page, and nothing of its health beside it"
port=$(forward "$gateway" 8081)
[ "$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$port/session")" = 404 ] || fail "the gateway's address"
ok "the gateway holds no session yet, and says so"

say "The cluster refuses what the architecture doesn't list"
server_ip=$(kubectl --namespace "$NAMESPACE" get pod "$server" --output jsonpath='{.status.podIP}')
gateway_ip=$(kubectl --namespace "$NAMESPACE" get pod "$gateway" --output jsonpath='{.status.podIP}')
issuer_ip=$(kubectl --namespace "$NAMESPACE" get pods --selector app.kubernetes.io/component=issuer --output jsonpath='{.items[0].status.podIP}')

# something to reach that no rule of Away's covers: the program itself, in another namespace
kubectl --namespace "$OUTSIDE" apply -f - >/dev/null <<YAML
apiVersion: v1
kind: Pod
metadata:
  name: elsewhere
spec:
  automountServiceAccountToken: false
  securityContext:
    runAsNonRoot: true
    seccompProfile: {type: RuntimeDefault}
  containers:
    - name: elsewhere
      image: $IMAGE_REPOSITORY:$IMAGE_TAG
      imagePullPolicy: $IMAGE_PULL_POLICY
      args: [server]
      securityContext:
        allowPrivilegeEscalation: false
        readOnlyRootFilesystem: true
        capabilities: {drop: [ALL]}
YAML
kubectl --namespace "$OUTSIDE" wait --for=condition=Ready pod/elsewhere --timeout=120s >/dev/null
elsewhere_ip=$(kubectl --namespace "$OUTSIDE" get pod elsewhere --output jsonpath='{.status.podIP}')

probe "$OUTSIDE" outsider
probe "$NAMESPACE" stranger
for part in server gateway issuer; do
  probe "$NAMESPACE" "as-$part" "app.kubernetes.io/name: away, app.kubernetes.io/instance: away, app.kubernetes.io/component: $part"
done

allowed() { if reaches "$1" "$2" "$3"; then ok "allowed: $4"; else fail "refused, and it has to be allowed: $4"; fi; }
refused() {
  if reaches "$1" "$2" "$3"; then
    fail "allowed, and it has to be refused: $4. Either a rule is wrong, or this cluster does not enforce network rules."
  else
    ok "refused: $4"
  fi
}

# what is allowed, so that a refusal below means a rule and not a broken network
allowed "$OUTSIDE"   outsider    "$elsewhere_ip:8080" "a pod that no rule covers, to another one"
allowed "$OUTSIDE"   outsider    "$server_ip:8080"    "the way in, to the server's address"
allowed "$OUTSIDE"   outsider    "$gateway_ip:8081"   "the way in, to the gateway's address"
# into Away: nothing but the way in
refused "$OUTSIDE"   outsider    "$server_ip:9090"    "another namespace, to the server's measures"
refused "$OUTSIDE"   outsider    "$gateway_ip:9090"   "another namespace, to the gateway's measures"
refused "$OUTSIDE"   outsider    "$issuer_ip:9090"    "another namespace, to the issuer"
refused "$NAMESPACE" stranger    "$issuer_ip:9090"    "another pod of Away's own namespace, to the issuer"
# out of Away: nothing yet, for any part
refused "$NAMESPACE" as-server   "$elsewhere_ip:8080" "the server, to something outside Away"
refused "$NAMESPACE" as-gateway  "$elsewhere_ip:8080" "the gateway, to something outside Away"
refused "$NAMESPACE" as-issuer   "$elsewhere_ip:8080" "the issuer, to something outside Away"
# between the parts: nothing yet
refused "$NAMESPACE" as-gateway  "$issuer_ip:9090"    "the gateway, to the issuer"
refused "$NAMESPACE" as-gateway  "$server_ip:8080"    "the gateway, to the server"
refused "$NAMESPACE" as-server   "$gateway_ip:8081"   "the server, to the gateway"
refused "$NAMESPACE" as-issuer   "$server_ip:8080"    "the issuer, to the server"

say "Everything this checks holds"
