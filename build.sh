#!/bin/sh
set -eu
cd "$(dirname "$0")"
export CARGO_TARGET_DIR="$(pwd)/target"

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
  rustup target add wasm32-unknown-unknown
fi

cargo build --release --target wasm32-unknown-unknown

wasm_src() {
  crate="$1"
  for candidate in \
    "target/wasm32-unknown-unknown/release/${crate}.wasm" \
    "target/wasm32-unknown-unknown/release/lib${crate}.wasm"
  do
    if [ -f "$candidate" ]; then
      echo "$candidate"
      return 0
    fi
  done
  echo "missing wasm for $crate" >&2
  find target/wasm32-unknown-unknown/release -name '*.wasm' | head
  exit 1
}

pack() {
  dest="$1"
  shift
  mkdir -p "$dest"
  for crate in "$@"; do
    src="$(wasm_src "$crate")"
    cp "$src" "$dest/${crate}.wasm"
    echo "$dest/${crate}.wasm ($(wc -c < "$src") bytes)"
  done
}

pack exten nekobt seadex animetosho animetosho_new anisearch
pack letmegetabyte seadex animetosho nyaa piratebay subsplease tokyotosho
pack anitorrent nyaa animetosho seadex subsplease yameii toonshub

mkdir -p catalogs
for crate in nyaa sukebei piratebay seadex animetosho animetosho_new subsplease tokyotosho yameii toonshub nekobt anisearch; do
  src="$(wasm_src "$crate")"
  cp "$src" "catalogs/${crate}.wasm"
done

python3 - <<'PY'
import json
from pathlib import Path

root = Path(".")
mapping = {
    "exten": "exten.json",
    "letmegetabyte": "letmegetabyte.json",
    "anitorrent": "anitorrent.json",
}
for folder, named in mapping.items():
    items = json.loads((root / folder / "index.json").read_text())
    for item in items:
        item["update"] = f"./{named}"
        code = item.get("code", "")
        item["code"] = "./" + Path(code).name
    out = root / "catalogs" / named
    out.write_text(json.dumps(items, indent=2) + "\n")
    print(f"wrote {out}")
PY

rm -f ./*.wasm

echo
echo "Standalone repos:  exten/  letmegetabyte/  anitorrent/"
echo "Combined repo:     catalogs/  (exten.json, letmegetabyte.json, anitorrent.json)"
