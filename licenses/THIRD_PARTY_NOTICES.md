# Third-party notice status

This source package does not vendor third-party library source or binaries and does not contain font files. Dependency manifests reference Tauri, xterm.js, Svelte, Rust crates and other libraries. Their respective notices must be included when the actual user-side build bundles them.

No RustDesk source is copied or linked. Reference-only discussion does not relicense that project's AGPL source.

A complete transitive notice/SBOM cannot be truthfully generated before dependency resolution. Generate actual lockfiles, inspect each resolved package's LICENSE, perform security/license audit, and replace this checklist with complete notices before distribution. The source-level MIT license above does not override any third-party license.

## Optional native-media dependency added in 0.2

The optional rc-media feature references the gstreamer/gstreamer-webrtc/gstreamer-sdp Rust 0.24 crates and a separately installed Windows MSVC GStreamer runtime/development SDK (1.24+ target). No SDK binaries, plugins or fonts are included here.

GStreamer describes its library licensing as LGPL and warns that plugin/codec distribution needs separate review. Do not relabel the SDK or all plugins as MIT, assume all H.264 distribution is royalty-free, or copy the entire runtime without collecting the actual resolved notices. The exact linked DLLs, plugins, Rust crate licenses, dependency checksums and local installation artifacts must be inventoried before distribution. This file is a disclosure checklist, not a completed license audit or SBOM.

Primary reference: https://gstreamer.freedesktop.org/documentation/application-development/appendix/licensing.html

The ws and @xterm/headless development dependencies are for the explicit Windows E2E runner. Node is not added to the production host runtime. Resolve their real versions and include their notices wherever distributed.
