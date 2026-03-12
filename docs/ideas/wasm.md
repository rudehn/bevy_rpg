# WASM Browser Build

Deploy IRONVEIL as a browser-playable game via GitHub Pages, auto-built on every push to `main`.

---

## Live URL

```
https://rudehn.github.io/bevy_rpg/
```

Update `YOUR_USERNAME` once the repo is public and GitHub Pages is enabled.

---

## Build Tool: Trunk

[Trunk](https://trunkrs.dev/) is the standard Bevy WASM build tool. It handles compiling to `wasm32-unknown-unknown`, running `wasm-bindgen`, copying assets, and generating the HTML wrapper.

```bash
# Install
cargo install trunk

# Local dev server (hot-reload) at http://localhost:8080
trunk serve

# Production build → ./dist/
trunk build --release
```

---

## Required Project Files

### `index.html` (repo root)

Trunk uses this as the HTML shell:

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <title>IRONVEIL</title>
    <style>
      body { margin: 0; background: #000; }
      canvas { display: block; }
    </style>
  </head>
  <body></body>
</html>
```

### `Trunk.toml` (repo root)

```toml
[build]
# Must match the GitHub Pages subdirectory path
public_url = "/bevy_rpg/"
```

---

## `Cargo.toml` Changes

### 1. Split `file_watcher` to native-only

`file_watcher` is a native-only hot-reload feature that won't compile for WASM:

```toml
# Remove the top-level bevy dependency and replace with:

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
bevy = { version = "0.17.0", features = ["file_watcher"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
bevy = { version = "0.17.0" }
```

### 2. WASM-optimized release profile

```toml
[profile.wasm-release]
inherits = "release"
opt-level = "z"      # minimize binary size
lto = true
codegen-units = 1
```

Build with: `trunk build --release -- --profile wasm-release`

### 3. `web-sys` for localStorage

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
web-sys = { version = "0.3", features = ["Window", "Storage"] }
```

---

## Save / Load in the Browser

The native build writes saves to `saves/ironveil_save.ron` on disk. In WASM, the filesystem isn't available — instead use the browser's `localStorage`.

Wrap all file I/O in `src/save/mod.rs` with `#[cfg(target_arch = "wasm32")]` guards:

```rust
// --- Write ---
#[cfg(not(target_arch = "wasm32"))]
fn write_save_data(data: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all("saves")?;
    std::fs::write("saves/ironveil_save.ron", data)?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn write_save_data(data: &str) -> anyhow::Result<()> {
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok()?)
        .ok_or_else(|| anyhow::anyhow!("localStorage unavailable"))?;
    storage.set_item("ironveil_save", data)
        .map_err(|e| anyhow::anyhow!("{:?}", e))
}

// --- Read ---
#[cfg(not(target_arch = "wasm32"))]
fn read_save_data() -> Option<String> {
    std::fs::read_to_string("saves/ironveil_save.ron").ok()
}

#[cfg(target_arch = "wasm32")]
fn read_save_data() -> Option<String> {
    web_sys::window()?
        .local_storage().ok()??
        .get_item("ironveil_save").ok()?
}

// --- Delete ---
#[cfg(not(target_arch = "wasm32"))]
fn delete_save_data() {
    let _ = std::fs::remove_file("saves/ironveil_save.ron");
}

#[cfg(target_arch = "wasm32")]
fn delete_save_data() {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok()?)
    {
        let _ = storage.remove_item("ironveil_save");
    }
}
```

localStorage key: `"ironveil_save"`

---

## GitHub Actions: Auto-Deploy to GitHub Pages

**File: `.github/workflows/wasm.yml`**

```yaml
name: Deploy WASM to GitHub Pages

on:
  push:
    branches: [main]

permissions:
  contents: write

jobs:
  build-and-deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust + WASM target
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - name: Cache cargo registry
        uses: actions/cache@v4
        with:
          path: ~/.cargo
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Install Trunk
        run: cargo install trunk --locked

      - name: Build WASM
        run: trunk build --release

      - name: Deploy to GitHub Pages
        uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./dist
```

Every push to `main` triggers a build (~5–10 min) and deploys the output to the `gh-pages` branch. GitHub Pages serves from that branch automatically.

**One-time setup:** In the repo Settings → Pages, set Source to "Deploy from a branch" → `gh-pages` / `/ (root)`.

---

## README Changes

Add at the top of `README.md`:

```markdown
## Play in Browser

**[▶ Play IRONVEIL in your browser](https://YOUR_USERNAME.github.io/bevy_rpg/)**

Auto-updated on every push to `main`. Requires a modern desktop browser with WebGL2 (Chrome, Firefox, Edge).
```

Badge:
```markdown
[![Play Now](https://img.shields.io/badge/play-browser-brightgreen)](https://YOUR_USERNAME.github.io/bevy_rpg/)
```

---

## Dependency Compatibility Notes

| Crate | WASM status | Notes |
|-------|-------------|-------|
| `bevy 0.17` | ✅ Supported | Disable `file_watcher` feature |
| `bracket-lib` (fork) | ⚠️ Verify | Standard bracket-lib supports WASM; confirm the custom fork does too with `cargo check --target wasm32-unknown-unknown` |
| `bevy_light_2d` | ✅ Likely fine | Pure rendering crate |
| `bevy_save` | ⚠️ Needs work | Uses filesystem internally; may need to be feature-gated or replaced with the `#[cfg]` save approach above |
| `bevy_common_assets` | ✅ Supported | RON asset loading works in WASM |
| `petgraph` / `rand` / `serde` / `ron` | ✅ Supported | Pure Rust, no OS dependencies |

---

## Local Testing Checklist

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk

# Verify it compiles
cargo check --target wasm32-unknown-unknown

# Dev server
trunk serve
# → open http://localhost:8080 in browser

# Production build
trunk build --release
# → inspect ./dist/ for size; target < 50 MB uncompressed
```
