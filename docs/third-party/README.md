# Third-party notices and package inventory

RemoteCodex's own source is covered by the repository [MIT license](../../LICENSE).
Dependency licenses remain with their respective authors.

`scripts/package-windows.ps1` collects the resolved Rust/npm inventory, installed
package license files, GStreamer SDK notices, and hashes of the staged application
files. `runtime/package/SBOM.json` uses CycloneDX 1.6. Its inventory includes build
and platform-specific dependencies; inclusion does not mean every dependency is
shipped or loaded at runtime. `licenses/inventory.json` records packages whose
license text was absent from the installed package archive.

`license-coverage.json` classifies those archive omissions by exact package
version. Source and staged notice hashes must both match before a supplemental
notice covers an entry. Of the 84 original omissions, 5 use the project license,
17 use verified shared upstream notices, and 60 are absent optional packages for
other targets. Two upstream texts remain unresolved: `is-reference@3.0.3` and
`locate-character@3.0.0`. An MIT label in upstream metadata is not substituted for
a missing copyright/license text. The staged inventory retains both the original
omissions and their coverage classification.

The supplemental files in `licenses/` cover upstream workspace licenses omitted
from some crate archives. [license-sources.json](license-sources.json) records each
download URL and SHA-256. Rust upstream revisions come from the installed crate's
`.cargo_vcs_info.json`; the MPL 2.0 text comes from Mozilla.

| Supplemental directory | Applicable package family |
| --- | --- |
| alloc-stdlib-0.2.4 | alloc-stdlib 0.2.4 |
| defmt-parser-1.0.0 | defmt-parser 1.0.0 |
| selectors-0.36.1 | selectors 0.36.1, MPL 2.0 text |
| tauri-plugin-2.6.3 | tauri-plugin 2.6.3 |
| unic-0.9.0 | unic-char-property, unic-char-range, unic-common, unic-ucd-version 0.9.0 |
| unic-ucd-ident-0.9.0 | unic-ucd-ident 0.9.0 |
| webview2-com-0.38.2 | webview2-com and webview2-com-sys 0.38.2 |
| webview2-com-macros-0.8.1 | webview2-com-macros 0.8.1 |

The native media package uses separate GStreamer DLLs/plugins. Their SDK notices
are retained under `licenses/gstreamer`; that notice collection includes SDK
components beyond the selected runtime plugin set. The staged file hashes and
`scripts/stage-media-runtime.ps1` identify the selected files. Upstream source and
build material are available from the [GStreamer project](https://gstreamer.freedesktop.org/)
and its [Cerbero build repository](https://gitlab.freedesktop.org/gstreamer/cerbero).
The runtime DLLs remain separate from the application executable.

The package inventory and collected texts are distribution evidence. They do not
certify an installer signature, codec patent clearance, or an unperformed license
review. Release verification must also check the actual installer contents and
the applicable Microsoft runtime redistribution terms.
