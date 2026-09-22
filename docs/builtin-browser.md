# Built-in browser surface

AgentCabin's desktop browser panel has one execution surface:

- The embedded surface is a real Electron `WebContentsView`. It paints in the
  application window, so the user and the Agent operate on the same Chromium
  page. Its page-level CDP session is exposed only through a token-authenticated
  loopback NDJSON relay owned by the Electron main process.
- The Browser Worker is only a small native-CDP adapter. It never launches a
  second browser, imports Playwright, or downloads a headless Chromium.

The address bar still calls Rust's browser interaction path. This keeps URL
validation and SSRF policy in the existing Rust authority boundary; the
Electron view only applies a confirmation dialog to unknown hosts reached by a
page-internal link. Loopback, private-network, and metadata hosts remain
blocked by the navigation policy.

Native views paint above renderer HTML. When the inspector opens a screenshot
preview or another supported overlay, the surface is shrunk to zero bounds so
the overlay can be interacted with. Closing the overlay restores the last
reported bounds. Other native or application-level overlays must follow the
same `visible` contract.

Bounds are reported in renderer CSS pixels and converted to Electron device
independent pixels using the window zoom factor. Window resize, scroll, and
host resize are observed, but OS-level display changes should still be checked
manually because DPI and display scale can vary across monitors.

The embedded surface is required. If its relay cannot be attached, the browser
operation fails explicitly instead of silently opening a second or screenshot
browser. This keeps the user and Agent on the same Chromium page.

The embedded worker deliberately supports page-level operations that map cleanly
to CDP: navigate, snapshot, screenshot, coordinate/ref click, text entry,
scroll, back/forward, reload, and the corresponding basic interaction actions.
Unsupported actions such as select-option, multi-tab creation/switching, and
locator semantics that cannot be expressed through the page-level CDP return an
`unsupported_action` error instead
of silently operating on a different browser.
