# Deploying

StarSheets is a static site: serve the `dist/` folder with any web server.

- WebAssembly must be served as `application/wasm` and should be gzip-compressed.
- Hashed files (`*-<hash>.js`, `*_bg.wasm`) can be cached forever; `index.html` should not be cached.
- Use HTTPS (or localhost): the browser's clipboard and file APIs need a secure context, and WebGPU is only offered there (otherwise WebGL2 is used).

A ready-made option: take the `starsheets-web` artifact from the latest green CI run on `main` and serve it with nginx.
