#!/usr/bin/env bash
# Lifecycle controller for the Jacquard e2e harness.
#
# Usage:
#   scripts/e2e.sh <tranquil|reference> [--keep] [--digest <sha256:...>]
#
# Environment:
#   JACQUARD_E2E_TRANQUIL_DIGEST / JACQUARD_E2E_REFERENCE_DIGEST
#     Explicit provider digest overrides (bypass tag resolution; recorded as
#     the effective digest so a run is reproducible).
#
# Providers use Docker's default bridge with no published PDS ports. The
# host-side transport and ingress allowlist fixture hosts. On failure or signal,
# sanitized diagnostics are collected into
# target/e2e/<run-id>/ before teardown unless --keep is passed.
set -Eeuo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$REPO_ROOT"

die() { echo "e2e: $*" >&2; exit 1; }
log() { printf '\033[1;34m[e2e]\033[0m %s\n' "$*" >&2; }

PROVIDER=${1:-}; shift || true
KEEP=0
DIGEST_OVERRIDE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --keep) KEEP=1 ;;
    --digest) DIGEST_OVERRIDE=$2; shift ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

case "$PROVIDER" in
  tranquil)
    TAG="atcr.io/tranquil.farm/tranquil-pds:latest"
    OVERRIDE_VAR=JACQUARD_E2E_TRANQUIL_DIGEST
    ;;
  reference)
    TAG="ghcr.io/bluesky-social/atproto:pds-spaces-alpha"
    OVERRIDE_VAR=JACQUARD_E2E_REFERENCE_DIGEST
    ;;
  *) die "usage: scripts/e2e.sh <tranquil|reference> [--keep] [--digest <sha256:...>]" ;;
esac
[ -n "$DIGEST_OVERRIDE" ] || DIGEST_OVERRIDE=${!OVERRIDE_VAR:-}

# ── tool validation ─────────────────────────────────────────────────────────
for tool in docker jq curl openssl cargo python3; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done
docker buildx version >/dev/null 2>&1 || die "docker buildx plugin not available"
docker compose version >/dev/null 2>&1 || die "docker compose v2 plugin not available"
docker info >/dev/null 2>&1 || die "docker daemon unreachable"
DOCKER_ROOTLESS=$(docker info --format '{{.SecurityOptions}}' 2>/dev/null || true)
case "$DOCKER_ROOTLESS" in *rootless*) die "rootless docker is unsupported: the native ingress must bind the bridge gateway" ;; esac

# ── run identity ────────────────────────────────────────────────────────────
RUN_ID="jqe2e-$(date -u +%Y%m%d%H%M%S)-$RANDOM"
ARTIFACT_DIR="$REPO_ROOT/target/e2e/$RUN_ID"
FIXTURE_ROOT="$ARTIFACT_DIR/fixtures"
mkdir -p "$FIXTURE_ROOT/identities" "$FIXTURE_ROOT/$PROVIDER"

compose() {
  # Project name is passed explicitly: `name:` interpolation from --env-file
  # is not applied for project identity in Compose v5.
  docker compose -p "$RUN_ID" --env-file "$FIXTURE_ROOT/compose.env" -f e2e/compose.yml "$@"
}

cleanup() {
  local rc=$?
  trap - INT TERM EXIT
  if [ $rc -ne 0 ] || [ $KEEP -eq 1 ]; then
    log "collecting diagnostics into $ARTIFACT_DIR"
    {
      compose ps -a --no-trunc
      echo; compose config 2>/dev/null \
        | sed -E 's/(PASSWORD|SECRET|KEY|TOKEN)[=:][^,}" ]+/\1=<redacted>/gI'
      for svc in tranquil-pds tranquil-db reference-pds e2e-dns; do
        compose logs --no-color --tail 500 "$svc" \
          > "$ARTIFACT_DIR/$svc.log" 2>&1 || true
      done
    } > "$ARTIFACT_DIR/ps.txt" 2>&1 || true
  fi
  [ -n "${INGRESS_PID:-}" ] && kill "$INGRESS_PID" 2>/dev/null || true
  if [ $KEEP -eq 1 ]; then
    log "--keep: leaving resources for inspection (project $RUN_ID)"
  else
    # `down` only removes services in enabled profiles; enable both so a
    # partial failure in one provider still tears everything down.
    compose --profile tranquil --profile reference down -v --remove-orphans >/dev/null 2>&1 || true
  fi
  exit $rc
}
trap cleanup INT TERM EXIT

# ── provider image resolution ───────────────────────────────────────────────
resolve_digest() {
  local tag=$1
  local descriptor manifest index_digest platform_digest
  if ! descriptor=$(docker buildx imagetools inspect --format '{{json .Manifest}}' "$tag" 2>>"$ARTIFACT_DIR/registry-errors.log"); then
    die "could not resolve $tag anonymously. If the registry requires auth (atcr.io needs 'docker login atcr.io' with an ATProto handle + app-password), log in and retry. Raw registry errors: $(tail -3 "$ARTIFACT_DIR/registry-errors.log" 2>/dev/null | tr '\n' ' ')"
  fi
  printf '%s\n' "$descriptor" > "$ARTIFACT_DIR/$(echo "$tag" | tr '/:@' '_').descriptor.json"
  index_digest=$(jq -r '.digest // empty' <<<"$descriptor")
  [ -n "$index_digest" ] || die "registry descriptor for $tag did not contain a digest"
  manifest=$(jq -c --arg arch "$ARCH" '[.manifests[]? | select(.platform.os == "linux" and .platform.architecture == $arch and (.digest | strings | length > 0))] | first' <<<"$descriptor")
  platform_digest=$(jq -r '.digest // empty' <<<"$manifest")
  printf '%s\n' "$descriptor" > "$ARTIFACT_DIR/$(echo "$tag" | tr '/:@' '_').index.json"
  printf '%s\n' "{\"index_digest\":\"$index_digest\",\"platform_digest\":$(jq -Rn --arg value "$platform_digest" '$value')}" \
    > "$ARTIFACT_DIR/$(echo "$tag" | tr '/:@' '_').digests.json"
  RESOLVED_INDEX_DIGEST=$index_digest
  RESOLVED_PLATFORM_DIGEST=$platform_digest
}

ARCH=$(docker version --format '{{.Server.Arch}}')
case "$ARCH" in
  amd64|arm64) : ;;
  *) die "unsupported host architecture: $ARCH" ;;
esac

if [ -n "$DIGEST_OVERRIDE" ]; then
  EFFECTIVE_DIGEST=$DIGEST_OVERRIDE
  EFFECTIVE_PLATFORM_DIGEST=$DIGEST_OVERRIDE
  EFFECTIVE_INDEX_DIGEST=""
  DIGEST_OVERRIDDEN=true
  log "provider digest override: $EFFECTIVE_DIGEST"
else
  resolve_digest "$TAG"
  EFFECTIVE_DIGEST=$RESOLVED_INDEX_DIGEST
  EFFECTIVE_PLATFORM_DIGEST=$RESOLVED_PLATFORM_DIGEST
  EFFECTIVE_INDEX_DIGEST=$RESOLVED_INDEX_DIGEST
  DIGEST_OVERRIDDEN=false
  log "resolved $TAG -> $EFFECTIVE_DIGEST ($(date -u +%FT%TZ))"
fi
echo "{\"tag\":\"$TAG\",\"effective_digest\":\"$EFFECTIVE_DIGEST\",\"effective_platform_digest\":\"$EFFECTIVE_PLATFORM_DIGEST\",\"overridden\":$DIGEST_OVERRIDDEN,\"resolved_at_utc\":\"$(date -u +%FT%TZ)\"}" > "$ARTIFACT_DIR/provider-image.json"

case "$PROVIDER" in
  tranquil) IMAGE="atcr.io/tranquil.farm/tranquil-pds@$EFFECTIVE_DIGEST" ;;
  reference) IMAGE="ghcr.io/bluesky-social/atproto@$EFFECTIVE_DIGEST" ;;
esac

docker pull --platform "linux/$ARCH" "$IMAGE" >/dev/null 2>&1 || die "could not pull $IMAGE for linux/$ARCH"
PULLED_REPO_DIGEST=$(docker image inspect --format '{{index .RepoDigests 0}}' "$IMAGE" 2>/dev/null || true)
case "$PULLED_REPO_DIGEST" in
  "$IMAGE") : ;;
  *) die "pulled image did not retain the requested digest: requested $IMAGE, got ${PULLED_REPO_DIGEST:-<none>}" ;;
esac

# ── per-run TLS material ────────────────────────────────────────────────────
HOSTS=(tranquil-identity.jacquard-e2e.test tranquil-member.jacquard-e2e.test
       reference-identity.jacquard-e2e.test reference-member.jacquard-e2e.test
       localhost.jacquard-e2e.test
       primary.tranquil.jacquard-e2e.test member.tranquil.jacquard-e2e.test
       primary.reference.jacquard-e2e.test member.reference.jacquard-e2e.test
       pds.tranquil.jacquard-e2e.test pds.reference.jacquard-e2e.test
       client.jacquard-e2e.dev service.jacquard-e2e.dev)
SAN=""
for h in "${HOSTS[@]}"; do SAN+="DNS:$h,"; done
SAN=${SAN%,}
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -keyout "$FIXTURE_ROOT/e2e-ca.key" \
  -out "$FIXTURE_ROOT/e2e-ca.pem" -days 2 -nodes -subj "/CN=jacquard-e2e ephemeral CA" \
  -addext "basicConstraints=critical,CA:TRUE" >/dev/null 2>&1
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -keyout "$FIXTURE_ROOT/ingress.key" \
  -out "$FIXTURE_ROOT/ingress.csr" -nodes -subj "/CN=client.jacquard-e2e.dev" >/dev/null 2>&1
openssl x509 -req -in "$FIXTURE_ROOT/ingress.csr" -CA "$FIXTURE_ROOT/e2e-ca.pem" -CAkey "$FIXTURE_ROOT/e2e-ca.key" \
  -CAcreateserial -out "$FIXTURE_ROOT/ingress.pem" -days 2 -extfile <(printf 'subjectAltName=%s\n' "$SAN") >/dev/null 2>&1

# ── coordinates ─────────────────────────────────────────────────────────────
# The host firewall accepts bridge→host traffic only from docker0, so the
# native ingress binds the docker0 gateway and services attach to the default
# bridge (`network_mode: bridge`). Service IPs are discovered after start.
E2E_GATEWAY=$(docker network inspect bridge --format '{{range .IPAM.Config}}{{.Gateway}}{{end}}')
[ -n "$E2E_GATEWAY" ] || die "could not inspect the docker0 gateway address"
INGRESS_PORT=$(python3 - <<'EOF'
import socket
s = socket.socket(); s.bind(("127.0.0.1", 0))
print(s.getsockname()[1]); s.close()
EOF
)
# Plain-HTTP ingress listener: Tranquil's did:web loopback exception fetches
# external documents over HTTP on 127.0.0.1 (the bridge gateway).
INGRESS_HTTP_PORT=$(python3 - <<'EOF'
import socket
s = socket.socket(); s.bind(("127.0.0.1", 0))
print(s.getsockname()[1]); s.close()
EOF
)

# Initial DNS config; rewritten after service IPs are discovered (webproc
# hot-reloads the mounted file).
# No `listen-address`: dnsmasq with 0.0.0.0 drops every query, and listing
# one address excludes the others.
write_dns_config() {
  local proxy_ip=$1 pds_ip=$2
  {
    echo "no-resolv"
    echo "no-hosts"
    echo "address=/plc.directory/"
    echo "address=/plc.invalid/"
    # Everything — identity hosts, handle hosts, and the PDS's advertised
    # https:// service endpoint — resolves to the socat passthrough, which
    # forwards 443 to the native ingress. The ingress terminates TLS and
    # routes by Host header (serving fixture documents itself, reverse-
    # proxying pds.* to the PDS's plain HTTP port).
    for h in "${HOSTS[@]}"; do echo "address=/$h/$proxy_ip"; done
    # Handle validation resolves _atproto.<handle> TXT back to the DID. The
    # record value is the full DID string — including the `did:` scheme —
    # or the PDS's handle→DID comparison fails.
    echo "txt-record=_atproto.primary.reference.jacquard-e2e.test,\"did=did:web:reference-identity.jacquard-e2e.test\""
    echo "txt-record=_atproto.member.reference.jacquard-e2e.test,\"did=did:web:reference-member.jacquard-e2e.test\""
    echo "txt-record=_atproto.primary.tranquil.jacquard-e2e.test,\"did=did:web:tranquil-identity.jacquard-e2e.test\""
    echo "txt-record=_atproto.member.tranquil.jacquard-e2e.test,\"did=did:web:tranquil-member.jacquard-e2e.test\""
    # Lexicon authority for the test space type NSID
    # `dev.jacquard.e2e.space`: the NSID's authority reverses to
    # `e2e.jacquard.dev` (NSID authorities are reversed name labels), so
    # lexicon resolution looks up `_lexicon.e2e.jacquard.dev` and expects
    # `did=<publisher DID>`. The declaration record itself is published into
    # that identity's repo by the spaces scenario.
    echo "txt-record=_lexicon.e2e.jacquard.dev,\"did=did:web:reference-identity.jacquard-e2e.test\""
  } > "$FIXTURE_ROOT/dnsmasq.conf"
}
write_dns_config 127.0.0.1 127.0.0.1

# Handle resolution files: Jacquard fetches
# https://{handle}/.well-known/atproto-did (text/plain DID) when configured
# with the HttpsWellKnown handle step.
mkdir -p "$FIXTURE_ROOT/handles"
printf 'did:web:reference-identity.jacquard-e2e.test' > "$FIXTURE_ROOT/handles/primary.reference.jacquard-e2e.test"
printf 'did:web:reference-member.jacquard-e2e.test' > "$FIXTURE_ROOT/handles/member.reference.jacquard-e2e.test"
printf 'did:web:tranquil-identity.jacquard-e2e.test' > "$FIXTURE_ROOT/handles/primary.tranquil.jacquard-e2e.test"
printf 'did:web:tranquil-member.jacquard-e2e.test' > "$FIXTURE_ROOT/handles/member.tranquil.jacquard-e2e.test"

# ── deterministic test-only secrets (never production material) ─────────────
printf 'Jacquard-E2E-%s-Admin7' "$RUN_ID" > "$FIXTURE_ROOT/$PROVIDER/admin-password"
openssl rand -hex 24 > "$FIXTURE_ROOT/$PROVIDER/app-password"
openssl rand -hex 24 > "$FIXTURE_ROOT/$PROVIDER/member-app-password"

cat > "$FIXTURE_ROOT/compose.env" <<EOF
E2E_RUN_ID=$RUN_ID
E2E_DNS_IP=pending
E2E_GATEWAY=$E2E_GATEWAY
E2E_INGRESS_PORT=$INGRESS_PORT
E2E_INGRESS_HTTP_PORT=$INGRESS_HTTP_PORT
E2E_FIXTURE_ROOT=$FIXTURE_ROOT
E2E_REFERENCE_IMAGE=$( [ "$PROVIDER" = reference ] && echo "$IMAGE" || echo "ghcr.io/bluesky-social/atproto@sha256:0000000000000000000000000000000000000000000000000000000000000000" )
E2E_TRANQUIL_IMAGE=$( [ "$PROVIDER" = tranquil ] && echo "$IMAGE" || echo "atcr.io/tranquil.farm/tranquil-pds@sha256:0000000000000000000000000000000000000000000000000000000000000000" )
E2E_REFERENCE_ADMIN_PASSWORD=jacquard-e2e-$RUN_ID-admin
E2E_REFERENCE_ROTATION_KEY=0000000000000000000000000000000000000000000000000000000000000001
E2E_REFERENCE_JWT_SECRET=$(openssl rand -hex 24)
E2E_TRANQUIL_JWT_SECRET=$(openssl rand -hex 24)
E2E_TRANQUIL_DPOP_SECRET=$(openssl rand -hex 24)
E2E_TRANQUIL_MASTER_KEY=$(openssl rand -hex 24)
E2E_TRANQUIL_DATABASE_URL=pending
EOF

# ── native ingress ──────────────────────────────────────────────────────────
log "starting native ingress on $E2E_GATEWAY:$INGRESS_PORT"
INGRESS_CERT="$FIXTURE_ROOT/ingress.pem" \
INGRESS_KEY="$FIXTURE_ROOT/ingress.key" \
INGRESS_BIND="$E2E_GATEWAY" \
INGRESS_PORT="$INGRESS_PORT" \
INGRESS_FIXTURE_ROOT="$FIXTURE_ROOT" \
INGRESS_PROVIDER="$PROVIDER" \
INGRESS_HTTP_PORT="$INGRESS_HTTP_PORT" \
  cargo run -q -p jacquard-e2e --features e2e --bin ingress &
INGRESS_PID=$!
# The ingress binary may need compiling; wait until it actually serves before
# any bootstrap step can PUT a DID document at it.
for _ in $(seq 1 120); do
  if curl -sk "https://$E2E_GATEWAY:$INGRESS_PORT/e2e-health" 2>/dev/null | grep -q jacquard-e2e; then
    break
  fi
  sleep 1
done
curl -sk "https://$E2E_GATEWAY:$INGRESS_PORT/e2e-health" 2>/dev/null | grep -q jacquard-e2e \
  || die "native ingress did not become ready on $E2E_GATEWAY:$INGRESS_PORT"

# ── start support services ───────────────────────────────────────────────────
log "starting e2e support services (dns, ingress proxy)"
compose up -d --pull never e2e-dns e2e-ingress-proxy >/dev/null

# ── discover service IPs and finalize configuration ─────────────────────────
# No static addressing on the default bridge: start the support services,
# discover their addresses, then rewrite the DNS config (hot-reloaded by
# webproc) and bring up the provider with the discovered coordinates.
svc_ip() {
  # Profiled services are invisible to `compose ps` without --profile.
  compose --profile tranquil --profile reference ps -q "$1" 2>/dev/null \
    | head -1 \
    | xargs -r docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null
}
DNS_IP=$(svc_ip e2e-dns)
PROXY_IP=$(svc_ip e2e-ingress-proxy)
[ -n "$DNS_IP" ] && [ -n "$PROXY_IP" ] || die "could not discover dns/proxy container IPs"
# Guard against garbage discovery values before they poison the DNS config.
ip_re='^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$'
printf 'discovered DNS_IP=%s PROXY_IP=%s\n' "$DNS_IP" "$PROXY_IP" >> "$ARTIFACT_DIR/discovery.log"
[[ "$DNS_IP" =~ $ip_re ]] || die "discovered DNS_IP is not an IP: $DNS_IP"
[[ "$PROXY_IP" =~ $ip_re ]] || die "discovered PROXY_IP is not an IP: $PROXY_IP"
sed -i.bak "s|^E2E_DNS_IP=.*|E2E_DNS_IP=$DNS_IP|" "$FIXTURE_ROOT/compose.env"

if [ "$PROVIDER" = tranquil ]; then
  # Postgres first: its address feeds the PDS environment.
  compose --profile tranquil up -d --pull never tranquil-db >/dev/null
  for _ in $(seq 1 30); do
    healthy=$(docker inspect -f '{{.State.Health.Status}}' "$(compose ps -q tranquil-db)" 2>/dev/null || true)
    [ "$healthy" = healthy ] && break
    sleep 2
  done
  DB_IP=$(svc_ip tranquil-db)
  [ -n "$DB_IP" ] || die "could not discover tranquil-db IP"
  sed -i.bak "s|^E2E_TRANQUIL_DATABASE_URL=.*|E2E_TRANQUIL_DATABASE_URL=postgres://tranquil:tranquil@$DB_IP:5432/tranquil|" "$FIXTURE_ROOT/compose.env"
  compose --profile tranquil up -d --pull never tranquil-pds >/dev/null
else
  compose --profile reference up -d --pull never reference-pds >/dev/null
fi

# Readiness: poll the provider health endpoint. Tranquil is distroless (no
# shell inside), so probe from the host over the bridge.
case "$PROVIDER" in
  tranquil) PDS_SVC=tranquil-pds ;;
  reference) PDS_SVC=reference-pds ;;
esac
ready=0
for _ in $(seq 1 60); do
  PDS_IP_NOW=$(svc_ip "$PDS_SVC" 2>/dev/null || true)
  if [ -n "$PDS_IP_NOW" ] && curl -sf -m 2 "http://$PDS_IP_NOW:3000/xrpc/_health" 2>/dev/null | grep -q version; then
    ready=1
    break
  fi
  sleep 2
done
[ "$ready" = 1 ] || die "provider $PROVIDER did not become healthy; logs retained in $ARTIFACT_DIR"

PDS_IP=$(svc_ip "$(case "$PROVIDER" in tranquil) echo tranquil-pds ;; reference) echo reference-pds ;; esac)")
[ -n "$PDS_IP" ] || die "could not inspect provider container IP"
write_dns_config "$PROXY_IP" "$PDS_IP"
# dnsmasq only reads its config at startup; restart the sidecar to pick up
# the discovered addresses. The container keeps its bridge IP.
compose restart e2e-dns >/dev/null 2>&1 || compose up -d --pull never e2e-dns >/dev/null

# Hand the ingress the PDS upstream through a file (read per request) so no
# ingress restart is needed: restarting would orphan the provider's keep-alive
# TLS connections and surface as connection resets mid-run.
case "$PROVIDER" in
  tranquil) PDS_SVC=tranquil-pds ;;
  reference) PDS_SVC=reference-pds ;;
esac
printf '%s' "$PDS_IP:3000" > "$FIXTURE_ROOT/pds-upstream"
# Wait until the ingress actually proxies with the new upstream.
for _ in $(seq 1 60); do
  if curl -sk "https://pds.$PROVIDER.jacquard-e2e.test:$INGRESS_PORT/xrpc/_health" 2>/dev/null | grep -q version; then
    break
  fi
  sleep 1
done

# Wait until the DNS sidecar actually answers fixture records (racing the
# restart would negatively cache lookups inside the PDS).
case "$PROVIDER" in
  tranquil) DNS_PROBE_HOST=pds.tranquil.jacquard-e2e.test ;;
  reference) DNS_PROBE_HOST=pds.reference.jacquard-e2e.test ;;
esac
dns_ready=0
for _ in $(seq 1 20); do
  if dig +time=1 +tries=1 @"$DNS_IP" "$DNS_PROBE_HOST" A +short 2>/dev/null | grep -qE '[0-9]+\.'; then
    dns_ready=1
    break
  fi
  sleep 1
done
[ "$dns_ready" = 1 ] || die "fixture DNS did not become resolvable"

# Verify the running container actually uses the effective digest.
for cid in $(compose ps -q 2>/dev/null); do
  docker inspect --format '{{.Image}} {{.Name}}' "$cid" >> "$ARTIFACT_DIR/container-images.txt"
done

# ── export non-secret coordinates and run scenarios ─────────────────────────
# The host test process cannot use bridge DNS; address the PDS by its
# container IP.
export JACQUARD_E2E_PROVIDER="$PROVIDER"
export JACQUARD_E2E_RUN_ID="$RUN_ID"
export JACQUARD_E2E_PROVIDER_URL="http://$PDS_IP:3000"
export JACQUARD_E2E_EFFECTIVE_DIGEST="$EFFECTIVE_DIGEST"
export JACQUARD_E2E_INGRESS_HTTP_PORT="$INGRESS_HTTP_PORT"
export JACQUARD_E2E_PROXY_IP="$PROXY_IP"
export JACQUARD_E2E_FIXTURE_ROOT="$FIXTURE_ROOT"
export JACQUARD_E2E_ARTIFACT_DIR="$ARTIFACT_DIR"

log "running scenarios for provider $PROVIDER (digest $EFFECTIVE_DIGEST)"
if cargo nextest run -p jacquard-e2e --features "e2e,$PROVIDER" 2>&1 | tee "$ARTIFACT_DIR/nextest.log"; then
  log "success: all scenarios passed for $PROVIDER"
else
  rc=$?
  log "scenario failure (rc=$rc); diagnostics in $ARTIFACT_DIR"
  exit $rc
fi
