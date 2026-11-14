#!/usr/bin/env bash
# Checks what the first work package promises of an installation, on the cluster that
# kubectl points at:
#
#   - one command installs it, by someone with rights in one namespace and nowhere else,
#     into a namespace that takes only the strictest profile;
#   - its three parts say that they are well, give their measures, and write their lines;
#   - the server answers on its address and the gateway on its own;
#   - behind an ingress controller, Away answers on its address with a certificate of
#     one's own, and nothing but that controller reaches it;
#   - the cluster refuses every connection the architecture doesn't list.
#
# By itself it installs Away into a namespace it makes, and removes it again:
#
#   tests/install/kubernetes.sh
#
# With INSTALLED=1 it checks an installation that is already there, with rights in that
# namespace alone, and leaves it as it is:
#
#   INSTALLED=1 NAMESPACE=away ADDRESS=away.example.com WAY_IN=router tests/install/kubernetes.sh
#
# With INGRESS it installs Away behind the cluster's ingress controller, on an address
# and with a certificate that it makes for the test, from an authority of its own:
#
#   INGRESS=ingress/traefik INGRESS_CLASS=traefik tests/install/kubernetes.sh
#
# It needs kubectl, helm, curl, jq and openssl, and a cluster that enforces network rules.
#
#   NAMESPACE          where Away is installed                         (away-test)
#   RELEASE            the installation's name                         (away)
#   KEEP               set to 1 to leave what this made in place
# to install:
#   CHART              the chart: a folder, or oci://...               (the chart of this repository)
#   CHART_VERSION      a published chart's version
#   IMAGE_REPOSITORY   the image, without its tag                      (ghcr.io/nalet/away)
#   IMAGE_TAG          its tag                                         (main)
#   IMAGE_PULL_POLICY  IfNotPresent, or Never for an image the cluster already holds
#   INGRESS            the ingress controller's service, as namespace/name: Away is then
#                      installed behind it, and tried through it
#   INGRESS_CLASS      the ingress controller's class                  (the cluster's default)
# for an installation that is there:
#   ADDRESS            the address it answers on: the way in is then tried from outside
#   WAY_IN             who the way in lets through to Away's address: any (every
#                      namespace, as with an ingress that names nobody) or router (the
#                      cluster's router alone, as with a route)        (any)
set -euo pipefail

INSTALLED=${INSTALLED:-}
NAMESPACE=${NAMESPACE:-away-test}
RELEASE=${RELEASE:-away}
ADDRESS=${ADDRESS:-}
WAY_IN=${WAY_IN:-any}
INGRESS=${INGRESS:-}
INGRESS_CLASS=${INGRESS_CLASS:-}
IMAGE_REPOSITORY=${IMAGE_REPOSITORY:-ghcr.io/nalet/away}
IMAGE_TAG=${IMAGE_TAG:-main}
IMAGE_PULL_POLICY=${IMAGE_PULL_POLICY:-IfNotPresent}
OUTSIDE=${NAMESPACE}-outside
# a small image with curl in it, for the pods that try the connections
PROBE_IMAGE=quay.io/curl/curl:8.23.0@sha256:3fb7e13e5d179a845c97d78204f3f1064d1907037f26a3ebf91710571151fd93
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
CHART=${CHART:-$ROOT/chart}
CHART_VERSION=${CHART_VERSION:-}
WORK=$(mktemp -d)
FORWARDS=()

say() { printf '\n== %s\n' "$*"; }
ok() { printf '   ok   %s\n' "$*"; }
fail() { printf '   FAIL %s\n' "$*" >&2; exit 1; }

clean() {
  for pid in "${FORWARDS[@]:-}"; do [ -n "$pid" ] && kill "$pid" 2>/dev/null || true; done
  if [ "${KEEP:-}" != 1 ]; then
    if [ -n "$INSTALLED" ]; then
      kubectl --namespace "$NAMESPACE" delete pods --selector away-test=probe --ignore-not-found --wait=false >/dev/null 2>&1 || true
    else
      kubectl delete namespace "$NAMESPACE" "$OUTSIDE" --ignore-not-found --wait=false >/dev/null 2>&1 || true
    fi
  fi
  rm -rf "$WORK"
}
trap clean EXIT

# OpenShift gives every pod a user of the namespace's own; elsewhere the pods here name one
if kubectl api-resources --api-group=security.openshift.io --output name 2>/dev/null | grep -q securitycontextconstraints; then
  RUN_AS=""
else
  RUN_AS="runAsUser: 65532"
fi

# forward_to <namespace> <pod/name or service/name> <port>: a way to one port of a pod or
# of a service, on a port of this machine
forward_to() {
  local log="$WORK/forward-$1-${2//\//-}-$3.log"
  kubectl --namespace "$1" port-forward "$2" ":$3" >"$log" 2>&1 &
  FORWARDS+=($!)
  for _ in $(seq 1 50); do
    if grep -q 'Forwarding from 127.0.0.1:' "$log"; then
      sed -n 's/^Forwarding from 127\.0\.0\.1:\([0-9]*\) .*/\1/p' "$log" | head -1
      return 0
    fi
    sleep 0.2
  done
  fail "no way to port $3 of $2: $(cat "$log")"
}

# forward <pod> <port>: a way to one port of one of Away's pods
forward() { forward_to "$NAMESPACE" "pod/$1" "$2"; }

# pod <namespace> <name> <labels> <image> <what it runs> [pull policy] [apart]: a pod of
# this test, under the strictest profile. One that is "apart" never says it is ready, so
# that no service sends it a request: it is for pods that carry a part's labels, which a
# part's service would otherwise take for the part.
pod() {
  local ready='' waits_for='condition=Ready'
  if [ "${7:-}" = apart ]; then
    ready='readinessProbe: {exec: {command: ["false"]}, periodSeconds: 3600}'
    waits_for="jsonpath={.status.phase}=Running"
  fi
  kubectl --namespace "$1" apply -f - >/dev/null <<YAML
apiVersion: v1
kind: Pod
metadata:
  name: $2
  labels: {away-test: probe${3:+, $3}}
spec:
  automountServiceAccountToken: false
  securityContext:
    runAsNonRoot: true
    $RUN_AS
    seccompProfile: {type: RuntimeDefault}
  containers:
    - name: probe
      image: $4
      imagePullPolicy: ${6:-IfNotPresent}
      $5
      $ready
      securityContext:
        allowPrivilegeEscalation: false
        readOnlyRootFilesystem: true
        capabilities: {drop: [ALL]}
YAML
  kubectl --namespace "$1" wait --for="$waits_for" "pod/$2" --timeout=180s >/dev/null \
    || { kubectl --namespace "$1" describe "pod/$2" | tail -15 >&2; fail "the test's pod $2 did not start"; }
}

# probe <namespace> <name> [labels] [apart]: a pod that waits, to try connections from
probe() { pod "$1" "$2" "${3:-}" "$PROBE_IMAGE" 'command: [sleep, "3600"]' IfNotPresent "${4:-}"; }

# reaches <namespace> <pod> <address>: whether a connection from the pod is taken
reaches() {
  kubectl --namespace "$1" exec "$2" -- curl --silent --output /dev/null --max-time 4 "http://$3" >/dev/null 2>&1
}

part_pod() {
  kubectl --namespace "$NAMESPACE" get pods --selector "app.kubernetes.io/instance=$RELEASE,app.kubernetes.io/component=$1,away-test!=probe" \
    --field-selector status.phase=Running --output jsonpath='{.items[0].metadata.name}'
}

if [ -z "$INSTALLED" ]; then
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
  if KUBECONFIG="$WORK/installer.kubeconfig" kubectl auth can-i create customresourcedefinitions.apiextensions.k8s.io >/dev/null 2>&1; then
    fail "the installer has rights over the whole cluster, so this proves nothing"
  fi
  ok "the installer may act in $NAMESPACE and nowhere else"

  BEHIND=()
  if [ -n "$INGRESS" ]; then
    say "A certificate of one's own for Away's address, from an authority that exists for this test"
    ADDRESS=away.example.com
    WAY_IN=ingress
    openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=An authority of one's own" -addext basicConstraints=critical,CA:TRUE \
      -keyout "$WORK/authority.key" -out "$WORK/authority.pem" 2>/dev/null || fail "the authority"
    openssl req -newkey rsa:2048 -nodes -subj "/CN=$ADDRESS" -keyout "$WORK/address.key" -out "$WORK/address.csr" 2>/dev/null || fail "the request for a certificate"
    printf 'subjectAltName=DNS:%s\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n' "$ADDRESS" > "$WORK/address.ext"
    openssl x509 -req -in "$WORK/address.csr" -CA "$WORK/authority.pem" -CAkey "$WORK/authority.key" -CAcreateserial -days 2 \
      -extfile "$WORK/address.ext" -out "$WORK/address.pem" 2>/dev/null || fail "the certificate"
    KUBECONFIG="$WORK/installer.kubeconfig" kubectl create secret tls away-address --cert="$WORK/address.pem" --key="$WORK/address.key" >/dev/null \
      || fail "the installer can't keep the certificate in $NAMESPACE"
    ok "a certificate for $ADDRESS, kept in $NAMESPACE by the installer"
    BEHIND=(--set ingress.enabled=true --set address="$ADDRESS" --set ingress.tlsSecret=away-address
            --set "networkRules.wayInFrom[0].namespaceSelector.matchLabels.kubernetes\.io/metadata\.name=${INGRESS%%/*}")
    [ -z "$INGRESS_CLASS" ] || BEHIND+=(--set ingress.className="$INGRESS_CLASS")
  fi

  say "One command installs it"
  KUBECONFIG="$WORK/installer.kubeconfig" helm install "$RELEASE" "$CHART" ${CHART_VERSION:+--version "$CHART_VERSION"} --namespace "$NAMESPACE" \
    --set image.repository="$IMAGE_REPOSITORY" --set image.tag="$IMAGE_TAG" --set image.pullPolicy="$IMAGE_PULL_POLICY" \
    ${BEHIND[@]+"${BEHIND[@]}"} --wait --timeout 240s >/dev/null || {
      kubectl --namespace "$NAMESPACE" get pods,events >&2 || true
      fail "helm install"
    }
  ok "helm install of $CHART${CHART_VERSION:+ $CHART_VERSION}, with its three parts ready"
else
  say "An installation that is there, checked with rights in its namespace alone"
  if kubectl auth can-i create customresourcedefinitions.apiextensions.k8s.io >/dev/null 2>&1; then
    fail "whoever runs this has rights over the whole cluster, so this proves nothing"
  fi
  kubectl --namespace "$NAMESPACE" rollout status "deployment" --selector "app.kubernetes.io/instance=$RELEASE" --timeout 240s >/dev/null \
    || fail "its parts are not ready"
  ok "$RELEASE in $NAMESPACE, with its three parts ready"
fi

say "Each part says that it is well, gives its measures and writes its lines"
for part in server gateway issuer; do
  pod=$(part_pod "$part")
  [ -n "$pod" ] || fail "$part: no pod"
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
  held=$(kubectl --namespace "$NAMESPACE" get pod "$pod" --output jsonpath='{.spec.containers[0].securityContext.readOnlyRootFilesystem}/{.spec.automountServiceAccountToken}/{.spec.containers[0].securityContext.runAsUser}')
  case "$held" in
    true/false/0) fail "$part: it runs as root" ;;
    true/false/*) ;;
    *) fail "$part: it can write its own files, or holds a token for the cluster ($held)" ;;
  esac
  user=${held##*/}
  ok "$part: alive, ready, $(grep -c '^away_' "$WORK/$part.measures") measures, $(wc -l < "$WORK/$part.lines" | tr -d ' ') lines${user:+, as the user $user that the cluster gave it}"
done

# Whether Away's pages are at an address: the page, sent with the rule that lets a browser
# load Away's own files and nothing else, the script it names, and the server's word on
# who is signed in, which is nobody.
pages() {
  curl --silent --max-time 20 "$1/" > "$WORK/page" || return 1
  script=$(grep -o '/assets/[^"]*\.js' "$WORK/page" | head -n 1)
  [ -n "$script" ] || return 1
  [ "$(curl --silent --max-time 20 --output /dev/null --write-out '%{http_code}' "$1$script")" = 200 ] || return 1
  curl --silent --max-time 20 --head "$1/" | tr -d '\r' | grep -qi "^content-security-policy: default-src 'none'; script-src 'self'; " || return 1
  curl --silent --max-time 20 "$1/api/who" | jq --exit-status '.version != null and .signed_in == false' > /dev/null || return 1
}

say "The server answers on its address, and the gateway on its own"
server=$(part_pod server)
gateway=$(part_pod gateway)
port=$(forward "$server" 8080)
pages "http://127.0.0.1:$port" || fail "the server's page"
[ "$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$port/livez")" = 404 ] || fail "the server says whether it is well on the address Away is reached at"
ok "the server's page, and nothing of its health beside it"
port=$(forward "$gateway" 8081)
[ "$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$port/session")" = 404 ] || fail "the gateway's address"
ok "the gateway holds no session yet, and says so"

if [ -n "$ADDRESS" ]; then
  say "The way in, from outside the cluster, at https://$ADDRESS"
  if [ -n "$INGRESS" ] && [ -z "$INSTALLED" ]; then
    # the address leads to the ingress controller, and the one authority that is trusted
    # is the test's own: a certificate of the controller's would be refused
    https=$(forward_to "${INGRESS%%/*}" "service/${INGRESS##*/}" 443)
    http=$(forward_to "${INGRESS%%/*}" "service/${INGRESS##*/}" 80)
    mkdir -p "$WORK/home"
    printf 'connect-to = "%s:443:127.0.0.1:%s"\nconnect-to = "%s:80:127.0.0.1:%s"\ncacert = "%s"\n' \
      "$ADDRESS" "$https" "$ADDRESS" "$http" "$WORK/authority.pem" > "$WORK/home/.curlrc"
    export CURL_HOME="$WORK/home"
    # the controller takes a moment to learn of a new ingress and its certificate
    for _ in $(seq 1 30); do
      curl --silent --max-time 5 --output /dev/null "https://$ADDRESS/" && break
      sleep 2
    done
  fi
  curl --silent --show-error --max-time 20 "https://$ADDRESS/" > "$WORK/page" || fail "https://$ADDRESS/ gives no answer, or a certificate that is not trusted"
  pages "https://$ADDRESS" || fail "https://$ADDRESS/ is not the server's page"
  if [ -n "${CURL_HOME:-}" ]; then
    ok "the server's page, behind the ingress controller, over the certificate of one's own"
  else
    ok "the server's page, over a certificate this machine trusts"
  fi
  [ "$(curl --silent --max-time 20 "https://$ADDRESS/session")" = "No session is at this address." ] || fail "https://$ADDRESS/session does not lead to the gateway"
  ok "a session's address leads to the gateway"
  [ "$(curl --silent --max-time 20 --output /dev/null --write-out '%{http_code}' "https://$ADDRESS/metrics")" = 404 ] || fail "the measures are on the address Away is reached at"
  ok "no measures on Away's address"
  if curl --silent --max-time 20 --head "https://$ADDRESS/" | grep -qi '^set-cookie:'; then
    fail "the way in gives the browser a cookie of its own"
  fi
  ok "the way in gives the browser no cookie of its own"
  case "$(curl --silent --max-time 20 --output /dev/null --write-out '%{http_code} %{redirect_url}' "http://$ADDRESS/")" in
    30[1278]\ https://*) ok "what arrives without TLS is sent on to https" ;;
    *) fail "http://$ADDRESS/ is answered without TLS" ;;
  esac
  unset CURL_HOME
fi

say "The cluster refuses what the architecture doesn't list"
server_ip=$(kubectl --namespace "$NAMESPACE" get pod "$server" --output jsonpath='{.status.podIP}')
gateway_ip=$(kubectl --namespace "$NAMESPACE" get pod "$gateway" --output jsonpath='{.status.podIP}')
issuer_ip=$(kubectl --namespace "$NAMESPACE" get pod "$(part_pod issuer)" --output jsonpath='{.status.podIP}')
image=$(kubectl --namespace "$NAMESPACE" get pod "$server" --output jsonpath='{.spec.containers[0].image}')

# where the pods that no rule of Away's covers live: a namespace of their own, or, for an
# installation that is there, Away's own
if [ -n "$INSTALLED" ]; then other=$NAMESPACE; else other=$OUTSIDE; fi
# something to reach that no rule of Away's covers: the program itself
pod "$other" elsewhere "" "$image" 'args: [server]' "$IMAGE_PULL_POLICY"
elsewhere_ip=$(kubectl --namespace "$other" get pod elsewhere --output jsonpath='{.status.podIP}')
probe "$other" outsider
[ -n "$INSTALLED" ] || probe "$NAMESPACE" stranger
for part in server gateway issuer; do
  # in a part's place, as the rules see it, and apart from the part's service
  probe "$NAMESPACE" "as-$part" "app.kubernetes.io/name: away, app.kubernetes.io/instance: $RELEASE, app.kubernetes.io/component: $part" apart
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
allowed "$other" outsider "$elsewhere_ip:8080" "a pod that no rule covers, to another one"
if [ "$WAY_IN" = router ] || [ "$WAY_IN" = ingress ]; then
  if [ "$WAY_IN" = router ]; then lets="the cluster's router"; else lets="the ingress controller"; fi
  refused "$other" outsider "$server_ip:8080"  "a pod that is not $lets, to the server's address"
  refused "$other" outsider "$gateway_ip:8081" "a pod that is not $lets, to the gateway's address"
else
  allowed "$other" outsider "$server_ip:8080"  "the way in, to the server's address"
  allowed "$other" outsider "$gateway_ip:8081" "the way in, to the gateway's address"
fi
# into Away: nothing but the way in
refused "$other" outsider "$server_ip:9090"  "a pod that is not of Away, to the server's measures"
refused "$other" outsider "$gateway_ip:9090" "a pod that is not of Away, to the gateway's measures"
refused "$other" outsider "$issuer_ip:9090"  "a pod that is not of Away, to the issuer"
[ -n "$INSTALLED" ] || refused "$NAMESPACE" stranger "$issuer_ip:9090" "another pod of Away's own namespace, to the issuer"
# out of Away: nothing yet, for any part
refused "$NAMESPACE" as-server  "$elsewhere_ip:8080" "the server, to something outside Away"
refused "$NAMESPACE" as-gateway "$elsewhere_ip:8080" "the gateway, to something outside Away"
refused "$NAMESPACE" as-issuer  "$elsewhere_ip:8080" "the issuer, to something outside Away"
# between the parts: nothing yet
refused "$NAMESPACE" as-gateway "$issuer_ip:9090"    "the gateway, to the issuer"
refused "$NAMESPACE" as-gateway "$server_ip:8080"    "the gateway, to the server"
refused "$NAMESPACE" as-server  "$gateway_ip:8081"   "the server, to the gateway"
refused "$NAMESPACE" as-issuer  "$server_ip:8080"    "the issuer, to the server"

say "Everything this checks holds"
