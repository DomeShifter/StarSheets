# Running StarSheets on a server

## The easy way: Docker

See the README: `ghcr.io/domeshifter/starsheets` holds the app and its file server in one
container (port 8080, files in `/data`). Settings:

| Setting | What it does |
|---|---|
| `STARSHEETS_PASSWORD` | The setup code for the first (owner's) account. Without it, a code is made on first start and printed in the log. Not needed once the owner's account exists |
| `STARSHEETS_AUTH=off` | No sign-in, for a private network you trust: everyone is a guest |
| `LOCATION` | How the app tells people where files are kept ("Saved to …") |
| `KEEP` | How many earlier versions of each file to keep (default 6) |

Spreadsheets are ordinary `.xlsx` files, in a folder for each person (`<username>/`) and in
folders as people make them in the app. `.starsheets` holds the file and folder ids, versions,
sharing, accounts and sign-in sessions; each person's bin is `<username>/.bin`.

**Where files are kept.** By default, the top of `/data`. The server's owner can choose any folder
inside `/data` (at first setup, or later from their avatar › Where files are kept). StarSheets then
moves everything there (or switches to StarSheets files already in that folder), writes the choice
to `/data/.starsheets-root` and **restarts itself**, so run the container with a restart policy
(`--restart unless-stopped`, as in the README and `docker/compose.yml`). To offer more folders,
mount a bigger folder as `/data`.

## By hand

- The app is a static site (`dist/` from `trunk build --release`). Serve WebAssembly as
  `application/wasm`, compressed; hashed files can be cached forever, `index.html` should not be.
- `starsheets-server` (from `crates/server`, a single program) stores the files. Put it behind the
  same host name as the app under `/api/`, or let it serve the app too with `SITE=path/to/dist`.
  It reads the same settings as the Docker image. A folder used by the older `server/files.py`
  works unchanged.
- Live editing uses a WebSocket at `/api/live`: a reverse proxy must pass the `Upgrade` and
  `Connection` headers through (nginx: `proxy_http_version 1.1; proxy_set_header Upgrade
  $http_upgrade; proxy_set_header Connection "upgrade";`) and allow long-lived connections.
  Without it the app still works and saves the usual way, one tab at a time.
- Use HTTPS (or localhost): browsers only give full clipboard access to secure pages.
