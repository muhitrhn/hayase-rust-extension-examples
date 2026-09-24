# Hayase extensions (Rust)

Upload the compiled `.wasm` files with the catalogs. Users never build anything — they paste a JSON URL and the app downloads `code` the same way original Hayase downloads `.js`.

```sh
sh build.sh
```

That copies the modules into the publish folders. You only run it when changing extension source.

## Separate GitHub repos

Each folder is ready to push as-is (`index.json` + `.wasm`):

| Folder | Original catalog |
| --- | --- |
| [`exten/`](exten/) | https://exten.pages.dev/index.json |
| [`letmegetabyte/`](letmegetabyte/) | https://raw.githubusercontent.com/LetMeGetAByte/Hayase-Extensions/main/index.json |
| [`anitorrent/`](anitorrent/) | https://raw.githubusercontent.com/anh9000/anitorrent/main/hayase/index.json |

Push the folder, then users import:

`https://raw.githubusercontent.com/<you>/<repo>/main/index.json`

## Combined GitHub repo

[`catalogs/`](catalogs/) is one repo: named JSON files plus shared `.wasm` files.

```
https://raw.githubusercontent.com/<you>/<repo>/main/exten.json
https://raw.githubusercontent.com/<you>/<repo>/main/letmegetabyte.json
https://raw.githubusercontent.com/<you>/<repo>/main/anitorrent.json
```

## Local import

Paste the absolute path to any of those JSON files in Settings → Extensions.

Do not import the original JS catalog URLs. Their `code` fields still point at `.js` workers.

## Source

Crates live in `extensions/`. That workspace is only for rebuilding the modules.
