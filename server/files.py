#!/usr/bin/env python3
"""StarSheets file store: keeps spreadsheets as ordinary .xlsx files in one folder (STORE).

A small JSON API for the web app, served behind the same nginx as the app (under /api/):

  GET    /api/files                    list: [{id, name, modified, size, version}]
  POST   /api/files?name=N             new file (body = xlsx bytes) -> {id, name, version}
  GET    /api/files/ID                 the file's bytes (header ETag = version)
  PUT    /api/files/ID                 save (header If-Match = the version it was based on)
                                       -> {version}; 409 if the file changed since
  POST   /api/files/ID/rename?name=N   -> {name}
  DELETE /api/files/ID                 move to the bin (STORE/.bin)
  GET    /api/files/ID/versions        older copies: [{v, modified, size}]
  GET    /api/files/ID/versions/V      an older copy's bytes

File names stay readable ("Budget.xlsx"). A name that is taken gets " (2)", " (3)"…; the free name
is claimed with an exclusive create, so two saves at once can't take the same one. Each file
has a hidden id (STORE/.starsheets/index.json) so the app follows a file across renames; a file
added to the folder by hand gets an id when it is first listed. The last KEEP versions of each
file are kept in STORE/.starsheets/versions/ID/.

Standard library only. Settings: STORE (folder), KEEP (versions, default 6), PORT (8080).
"""

import json
import os
import re
import secrets
import shutil
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlparse

STORE = os.environ.get("STORE", "/data")
KEEP = int(os.environ.get("KEEP", "6"))
PORT = int(os.environ.get("PORT", "8080"))
META = os.path.join(STORE, ".starsheets")
INDEX = os.path.join(META, "index.json")
VERSIONS = os.path.join(META, "versions")
BIN = os.path.join(STORE, ".bin")
MAX_BYTES = 200 * 1024 * 1024
EXTS = (".xlsx", ".xlsm", ".xls", ".csv", ".ods")

lock = threading.Lock()


def load_index():
    try:
        with open(INDEX, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_index(index):
    os.makedirs(META, exist_ok=True)
    tmp = INDEX + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(index, f, ensure_ascii=False, indent=1)
    os.replace(tmp, INDEX)


def synced_index():
    """The index, brought in line with the folder: new files get ids, vanished ones are dropped."""
    index = load_index()
    names = {n for n in os.listdir(STORE) if n.lower().endswith(EXTS) and not n.startswith(".")}
    known = set(index.values())
    changed = False
    for fid, name in list(index.items()):
        if name not in names:
            del index[fid]
            changed = True
    for name in sorted(names - known):
        index[secrets.token_hex(8)] = name
        changed = True
    if changed:
        save_index(index)
    return index


def version_of(path):
    st = os.stat(path)
    return f"{st.st_mtime_ns:x}-{st.st_size:x}"


def clean_name(name):
    """A safe display name: no folders, no characters Windows/SMB refuse, not empty."""
    name = re.sub(r'[\\/:*?"<>|\x00-\x1f]', "-", (name or "").strip()).strip(". ")
    return name[:150] or "Untitled spreadsheet"


def claim_name(stem, ext, own=None):
    """Reserves a free file name (stem.ext, stem (2).ext, …) by creating it exclusively."""
    for n in range(1, 10000):
        name = f"{stem}{'' if n == 1 else f' ({n})'}{ext}"
        if name == own:
            return name
        try:
            fd = os.open(os.path.join(STORE, name), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o664)
            os.close(fd)
            return name
        except FileExistsError:
            continue
    raise RuntimeError("no free name")


def split_name(name):
    stem, ext = os.path.splitext(name)
    if ext.lower() not in EXTS:
        stem, ext = name, ".xlsx"
    return stem, ext


def write_file(path, data):
    tmp = os.path.join(META, f".{secrets.token_hex(6)}.tmp")
    os.makedirs(META, exist_ok=True)
    with open(tmp, "wb") as f:
        f.write(data)
    os.replace(tmp, path)


def keep_version(fid, path):
    """Copies the file as it is now into its versions folder, keeping the newest KEEP."""
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return
    folder = os.path.join(VERSIONS, fid)
    os.makedirs(folder, exist_ok=True)
    shutil.copyfile(path, os.path.join(folder, f"{time.strftime('%Y%m%d-%H%M%S')}-{secrets.token_hex(2)}.xlsx"))
    old = sorted(os.listdir(folder))
    for v in old[:-KEEP] if KEEP > 0 else old:
        os.remove(os.path.join(folder, v))


class Handler(BaseHTTPRequestHandler):
    server_version = "StarSheetsFiles/1"

    def log_message(self, fmt, *args):
        pass

    def send_json(self, obj, code=200, headers=None):
        body = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def fail(self, code, message):
        self.send_json({"error": message}, code)

    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        if n > MAX_BYTES:
            raise ValueError("file too large")
        return self.rfile.read(n)

    def route(self):
        u = urlparse(self.path)
        parts = [unquote(p) for p in u.path.split("/") if p]
        if parts[:2] != ["api", "files"]:
            return None, [], {}
        return parts[2:], parts, {k: v[0] for k, v in parse_qs(u.query).items()}

    def lookup(self, index, fid):
        name = index.get(fid)
        if not name:
            self.fail(404, "no such file")
            return None
        return name

    def do_GET(self):
        rest, _, _ = self.route()
        if rest is None:
            return self.fail(404, "not found")
        with lock:
            index = synced_index()
            if not rest:
                out = []
                for fid, name in index.items():
                    p = os.path.join(STORE, name)
                    st = os.stat(p)
                    out.append({"id": fid, "name": name, "modified": int(st.st_mtime), "size": st.st_size, "version": version_of(p)})
                out.sort(key=lambda f: -f["modified"])
                return self.send_json(out)
            name = self.lookup(index, rest[0])
            if not name:
                return
            path = os.path.join(STORE, name)
            if len(rest) == 1:
                data, version = open(path, "rb").read(), version_of(path)
            elif rest[1:2] == ["versions"]:
                folder = os.path.join(VERSIONS, rest[0])
                vs = sorted(os.listdir(folder), reverse=True) if os.path.isdir(folder) else []
                if len(rest) == 2:
                    return self.send_json([{"v": v, "modified": int(os.path.getmtime(os.path.join(folder, v))), "size": os.path.getsize(os.path.join(folder, v))} for v in vs])
                if rest[2] not in vs:
                    return self.fail(404, "no such version")
                data, version = open(os.path.join(folder, rest[2]), "rb").read(), ""
            else:
                return self.fail(404, "not found")
        self.send_response(200)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-File-Name", json.dumps(name))
        if version:
            self.send_header("ETag", version)
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        rest, _, q = self.route()
        if rest is None:
            return self.fail(404, "not found")
        try:
            data = self.body()
        except ValueError as e:
            return self.fail(413, str(e))
        with lock:
            index = synced_index()
            if not rest:
                stem, ext = split_name(clean_name(q.get("name")))
                name = claim_name(stem, ext)
                path = os.path.join(STORE, name)
                write_file(path, data)
                fid = secrets.token_hex(8)
                index[fid] = name
                save_index(index)
                return self.send_json({"id": fid, "name": name, "version": version_of(path)}, 201)
            if rest[1:] == ["rename"]:
                old = self.lookup(index, rest[0])
                if not old:
                    return
                stem, ext = split_name(clean_name(q.get("name")))
                _, old_ext = split_name(old)
                ext = old_ext or ext
                name = claim_name(stem, ext, own=old)
                if name != old:
                    os.replace(os.path.join(STORE, old), os.path.join(STORE, name))
                    index[rest[0]] = name
                    save_index(index)
                return self.send_json({"name": name})
        self.fail(404, "not found")

    def do_PUT(self):
        rest, _, _ = self.route()
        if not rest or len(rest) != 1:
            return self.fail(404, "not found")
        try:
            data = self.body()
        except ValueError as e:
            return self.fail(413, str(e))
        with lock:
            index = synced_index()
            name = self.lookup(index, rest[0])
            if not name:
                return
            path = os.path.join(STORE, name)
            want = self.headers.get("If-Match")
            if want and want != version_of(path):
                return self.send_json({"error": "changed elsewhere", "name": name}, 409)
            keep_version(rest[0], path)
            write_file(path, data)
            return self.send_json({"version": version_of(path), "name": name})

    def do_DELETE(self):
        rest, _, _ = self.route()
        if not rest or len(rest) != 1:
            return self.fail(404, "not found")
        with lock:
            index = synced_index()
            name = self.lookup(index, rest[0])
            if not name:
                return
            os.makedirs(BIN, exist_ok=True)
            stem, ext = split_name(name)
            target = os.path.join(BIN, f"{stem} ({time.strftime('%Y-%m-%d %H%M%S')}){ext}")
            shutil.move(os.path.join(STORE, name), target)
            del index[rest[0]]
            save_index(index)
            return self.send_json({"ok": True})


if __name__ == "__main__":
    os.makedirs(META, exist_ok=True)
    print(f"StarSheets files: {STORE} on :{PORT}, keeping {KEEP} versions", flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
