# Combined catalogs

Push **this folder** as a single GitHub repo. Each catalog is a named JSON file so you can import them separately without three repositories.

| File | Original |
| --- | --- |
| `exten.json` | https://exten.pages.dev/index.json |
| `letmegetabyte.json` | https://raw.githubusercontent.com/LetMeGetAByte/Hayase-Extensions/main/index.json |
| `anitorrent.json` | https://raw.githubusercontent.com/anh9000/anitorrent/main/hayase/index.json |

Upload the `.wasm` files next to these JSON files. Users import a catalog URL and never build. `code` paths are `./nyaa.wasm`, same idea as original `./nyaa.js`.

## Import

```
https://raw.githubusercontent.com/<you>/<repo>/main/exten.json
https://raw.githubusercontent.com/<you>/<repo>/main/letmegetabyte.json
https://raw.githubusercontent.com/<you>/<repo>/main/anitorrent.json
```
