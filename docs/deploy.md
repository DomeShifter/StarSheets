# Running StarSheets on a server

## The easy way: Docker

See the README: `ghcr.io/domeshifter/starsheets` holds the app and its file server in one
container (port 8080, files in `/data`). Settings:

| Setting | What it does |
|---|---|
| `STARSHEETS_PASSWORD` | The password to sign in. Without it, a random one is made on first start and printed in the log |
| `STARSHEETS_AUTH=off` | No login, for a private network you trust |
| `LOCATION` | How the app tells people where files are kept ("Saved to …") |
| `KEEP` | How many earlier versions of each file to keep (default 6) |

Spreadsheets are ordinary `.xlsx` files in `/data`, in folders as you make them in the app.
`/data/.starsheets` holds the file ids, versions and sign-in sessions, and `/data/.bin` the bin.

## By hand

- The app is a static site (`dist/` from `trunk build --release`). Serve WebAssembly as
  `application/wasm`, compressed; hashed files can be cached forever, `index.html` should not be.
- `starsheets-server` (from `crates/server`, a single program) stores the files. Put it behind the
  same host name as the app under `/api/`, or let it serve the app too with `SITE=path/to/dist`.
  It reads the same settings as the Docker image. A folder used by the older `server/files.py`
  works unchanged.
- Live editing (coming) will use a WebSocket under `/api/`: a reverse proxy must pass
  `Upgrade` and `Connection` headers through.
- Use HTTPS (or localhost): browsers only give full clipboard access to secure pages.
