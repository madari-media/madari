//! Keeps `cargo build`/`cargo test` working on a fresh clone, where the settings
//! UI has not been built yet.
//!
//! `src/web.rs` embeds the React bundle with `include_str!`, which is a compile
//! error when `web/dist` is absent. The real assets are produced by
//! `scripts/build-tv-native.sh` (or `npm ci && npm run build` inside `web/`), so
//! this writes an honest placeholder only when a file is genuinely missing and
//! warns loudly — a build must not fail with "couldn't read web/dist/index.html".
use std::{env, fs, path::Path};

/// Shown instead of the settings UI when someone runs a build without it.
const PLACEHOLDER_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>Madari TV settings</title>
  </head>
  <body style="font: 16px system-ui; padding: 2rem">
    <h1>Settings UI not built</h1>
    <p>Run <code>npm ci &amp;&amp; npm run build</code> in <code>crates/madari-tv/web</code>,
    or <code>./scripts/build-tv-native.sh</code>, then rebuild.</p>
  </body>
</html>
"#;

const PLACEHOLDERS: [(&str, &str); 3] = [
    ("index.html", PLACEHOLDER_HTML),
    (
        "app.js",
        "console.warn('Madari web settings were not built; rebuild the app.');\n",
    ),
    ("style.css", ""),
];

fn main() {
    // Rebuild the crate when the embedded bundle changes.
    println!("cargo:rerun-if-changed=web/dist/index.html");
    println!("cargo:rerun-if-changed=web/dist/app.js");
    println!("cargo:rerun-if-changed=web/dist/style.css");

    let Ok(manifest) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let dist = Path::new(&manifest).join("web/dist");
    let mut written = Vec::new();
    for (name, contents) in PLACEHOLDERS {
        let path = dist.join(name);
        if path.exists() {
            continue;
        }
        if fs::create_dir_all(&dist).is_err() || fs::write(&path, contents).is_err() {
            continue;
        }
        written.push(name);
    }
    if !written.is_empty() {
        println!(
            "cargo:warning=web/dist was missing ({}) and placeholders were written. \
             Run `npm ci && npm run build` in crates/madari-tv/web, or ./scripts/build-tv-native.sh, \
             before shipping a build.",
            written.join(", ")
        );
    }
}
