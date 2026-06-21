#!/usr/bin/env bash
set -euo pipefail

# Sync the on-device STT models (Parakeet TDT 0.6B v3 ASR + speaker diarizer):
#   1. download them from HuggingFace into ~/.ariso/models, reproducing the EXACT
#      on-disk layout FluidAudio (via the ariso-stt sidecar) writes at runtime,
#      and write a SHA256SUMS manifest over the downloaded files;
#   2. when R2 credentials are present, publish them to Cloudflare R2 under an
#      IMMUTABLE, content-addressed prefix:
#        models/<folder>/<short-sha>/<model-files>
#        e.g. models/parakeet-tdt-0.6b-v3/aed027400592/Encoder.mlmodelc/...
#
# <short-sha> is the HuggingFace commit the bytes came from, so each upstream
# revision lands at its own prefix and is never overwritten. This is Option B:
# a stable, verifiable origin to replace FluidAudio's unchecked, mutable
# resolve/main download. The Rust app then pulls from a prefix pinned at compile
# time (alongside per-file checksums).
#
# Layout written locally (matches a live install under ~/.ariso/models):
#   <models>/parakeet-tdt-0.6b-v3/  Preprocessor/Encoder/Decoder/JointDecisionv3 .mlmodelc + root *.json/*.txt
#   <models>/speaker-diarization/   pyannote_segmentation/wespeaker_v2 .mlmodelc           + root *.json/*.txt
# (FluidAudio strips the caller's leaf dir and lays each repo out under its own
# folder name at the models root — hence no asr/ or diarizer/ wrapper.)
#
# The selected file subset mirrors FluidAudio 0.14.8: AsrModels v3 with the
# default int8 encoder (plain Encoder.mlmodelc, NOT EncoderInt4) and the online
# DiarizerModels. On a FluidAudio bump, re-verify against ModelNames.swift.
#
# Optional environment:
#   MODELS_DIR       local dest (default ${ARISO_ROOT:-$HOME/.ariso}/models)
#   PARAKEET_REV / DIARIZER_REV   HF branch/tag/SHA (default main); pin to a SHA
#                    for a reproducible mirror — the resolved commit is printed
#   SHORT_SHA_LEN    R2 prefix length of the commit (default 12)
#   NO_UPLOAD=1      download only, never upload (even if R2 creds are set)
#   FORCE=1          overwrite an existing R2 prefix (defeats immutability)
#   R2_PUBLIC_BASE   public host for printed URLs (default the app's r2.dev)
#
# Upload requires (same as .github/scripts/release-publish.sh); absent => skipped:
#   R2_ENDPOINT   https://<account-id>.r2.cloudflarestorage.com
#   R2_BUCKET     bucket NAME backing the public pub-...r2.dev domain (no dots)
#   AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY    R2 API token credentials

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

HF_ENDPOINT="${HF_ENDPOINT:-https://huggingface.co}"
MODELS_DIR="${MODELS_DIR:-${ARISO_ROOT:-$HOME/.ariso}/models}"
SHORT_SHA_LEN="${SHORT_SHA_LEN:-12}"
R2_PUBLIC_BASE="${R2_PUBLIC_BASE:-https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev}"

PARAKEET_REPO="${PARAKEET_REPO:-FluidInference/parakeet-tdt-0.6b-v3-coreml}"
PARAKEET_REV="${PARAKEET_REV:-main}"
DIARIZER_REPO="${DIARIZER_REPO:-FluidInference/speaker-diarization-coreml}"
DIARIZER_REV="${DIARIZER_REV:-main}"

for bin in curl node shasum; do
  command -v "$bin" >/dev/null 2>&1 || { echo "Missing required tool: $bin" >&2; exit 1; }
done

# Decide whether to upload: only when all R2 vars are set and NO_UPLOAD isn't.
UPLOAD=1
for v in R2_ENDPOINT R2_BUCKET; do
  [[ -n "${!v:-}" ]] || UPLOAD=0
done
[[ "${NO_UPLOAD:-0}" == "1" ]] && UPLOAD=0

if [[ "$UPLOAD" == "1" ]]; then
  command -v aws >/dev/null 2>&1 || { echo "Upload needs the AWS CLI: brew install awscli" >&2; exit 1; }
  # R2_BUCKET must be the bucket NAME, not the public pub-<hash>.r2.dev domain.
  if [[ "$R2_BUCKET" == *.* ]]; then
    echo "R2_BUCKET looks like a domain ('${R2_BUCKET}'), not a bucket name." >&2
    exit 1
  fi
else
  echo "note: R2 credentials not set (or NO_UPLOAD=1) — download only, no upload." >&2
fi

mkdir -p "$MODELS_DIR"

# Resolve a branch/tag/SHA to its immutable commit SHA so the download and the
# R2 prefix are both reproducible.
resolve_sha() {
  local repo="$1" rev="$2" sha
  sha="$(curl -fsSL "${HF_ENDPOINT}/api/models/${repo}/revision/${rev}" \
    | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>process.stdout.write((JSON.parse(s).sha)||""))')"
  [[ -n "$sha" ]] || { echo "Could not resolve ${repo}@${rev}" >&2; exit 1; }
  printf '%s' "$sha"
}

# Emit "<size>\t<path>" for every repo file to download: anything under one of
# the given model-dir prefixes, plus root-level *.json / *.txt. Reproduces
# FluidAudio's selection (required .mlmodelc dirs + root metadata) over the
# recursive HF tree; nested json in NON-required dirs is excluded because
# FluidAudio only recurses into the required dirs. Args: repo rev prefix...
select_files() {
  local repo="$1" rev="$2"; shift 2
  curl -fsSL "${HF_ENDPOINT}/api/models/${repo}/tree/${rev}?recursive=true" \
    | PREFIXES="$*" node -e '
      let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{
        const items=JSON.parse(s);
        const prefixes=(process.env.PREFIXES||"").split(/\s+/).filter(Boolean);
        for(const it of items){
          if(it.type!=="file")continue;
          const p=it.path;
          const inDir=prefixes.some(pre=>p.startsWith(pre));
          const rootMeta=!p.includes("/")&&(p.endsWith(".json")||p.endsWith(".txt"));
          if(inDir||rootMeta)console.log(`${it.size||0}\t${p}`);
        }
      });'
}

# Write a deterministic SHA256SUMS over the downloaded files (paths relative to
# the folder), verifiable with `shasum -a 256 -c SHA256SUMS` from inside it. It
# is published alongside the model so the Rust downloader can verify every file
# against pinned hashes before writing the readiness manifest. Excludes the
# manifest itself plus any *.part / .DS_Store; sorted so reruns are byte-stable.
write_checksums() {
  local dest="$1"
  ( cd "$dest" && find . -type f \
      ! -name SHA256SUMS ! -name '*.part' ! -name '.DS_Store' -print0 \
      | LC_ALL=C sort -z \
      | xargs -0 shasum -a 256 \
      | sed 's|  \./|  |' ) > "$dest/SHA256SUMS"
}

# Download a repo subset into <models>/<folder>, then (if uploading) push it to
# the immutable R2 prefix. Args: repo rev folder prefix...
sync_model() {
  local repo="$1" rev="$2" folder="$3"; shift 3
  local sha short dest count=0
  sha="$(resolve_sha "$repo" "$rev")"
  short="${sha:0:${SHORT_SHA_LEN}}"
  dest="${MODELS_DIR}/${folder}"
  echo "==> ${repo}@${rev}"
  echo "    commit: ${sha}  (prefix ${short})"
  echo "    local:  ${dest}"
  mkdir -p "$dest"

  # 1) Download — re-runnable: files whose size already matches are skipped.
  while IFS=$'\t' read -r size path; do
    [[ -n "$path" ]] || continue
    local out="${dest}/${path}"
    mkdir -p "$(dirname "$out")"
    if [[ -f "$out" && "$size" != "0" ]]; then
      local have; have="$(stat -f%z "$out" 2>/dev/null || echo -1)"
      [[ "$have" == "$size" ]] && { count=$((count + 1)); continue; }
    fi
    if [[ "$size" == "0" ]]; then
      : > "$out"   # HF serves HTTP 500 for 0-byte files; create empty locally
    else
      curl -fsSL "${HF_ENDPOINT}/${repo}/resolve/${sha}/${path}" -o "${out}.part" \
        || { rm -f "${out}.part"; echo "failed: ${path}" >&2; exit 1; }
      mv -f "${out}.part" "$out"
    fi
    count=$((count + 1))
    printf '\r    files: %d' "$count"
  done < <(select_files "$repo" "$sha" "$@")
  printf '\r    files: %d (done)\n' "$count"
  [[ "$count" -gt 0 ]] || {
    echo "No files matched for ${repo} — the file set may have changed; re-verify against ModelNames.swift" >&2
    exit 1
  }

  # 2) Write the checksum manifest over what was downloaded.
  write_checksums "$dest"
  echo "    sums:   ${dest}/SHA256SUMS ($(wc -l < "$dest/SHA256SUMS" | tr -d ' ') files)"

  # 3) Upload to the immutable, content-addressed prefix (aws s3 only).
  if [[ "$UPLOAD" == "1" ]]; then
    local key="models/${folder}/${short}" existing
    echo "    r2:     s3://${R2_BUCKET}/${key}/"
    existing="$(aws s3 ls "s3://${R2_BUCKET}/${key}/" --endpoint-url "$R2_ENDPOINT" 2>/dev/null || true)"
    if [[ -n "$existing" && "${FORCE:-0}" != "1" ]]; then
      echo "    prefix already exists — treating as immutable. Set FORCE=1 to overwrite." >&2
      exit 1
    fi
    aws s3 cp "$dest" "s3://${R2_BUCKET}/${key}/" --recursive \
      --exclude "*.DS_Store" --endpoint-url "$R2_ENDPOINT"
    echo "    public: ${R2_PUBLIC_BASE}/${key}/"
    echo "    pin:    ${folder} -> ${short}"
  fi
}

sync_model "$PARAKEET_REPO" "$PARAKEET_REV" "parakeet-tdt-0.6b-v3" \
  "Preprocessor.mlmodelc/" "Encoder.mlmodelc/" "Decoder.mlmodelc/" "JointDecisionv3.mlmodelc/"

sync_model "$DIARIZER_REPO" "$DIARIZER_REV" "speaker-diarization" \
  "pyannote_segmentation.mlmodelc/" "wespeaker_v2.mlmodelc/"

echo
if [[ "$UPLOAD" == "1" ]]; then
  echo "Synced to ${MODELS_DIR} and R2. Pin the per-folder <short-sha> prefixes above in the Rust downloader."
else
  echo "Downloaded to ${MODELS_DIR} (each model folder has a SHA256SUMS). Set R2_* credentials to also publish the immutable mirror."
fi
