# macOS Tao/Wry Pump Fixture

This standalone crate compares the same Tao window and Wry WKWebView under the
externally driven macOS event loop and the normal blocking event loop.

```sh
cargo run -- --mode pump --duration-ms 2200
cargo run -- --mode blocking --duration-ms 2200
```

The pump mode exits unsuccessfully unless WKWebView finishes navigation and
both the initial and mutated DOM can be evaluated. It also supports the
run-loop boundary controls used by the regression investigation:

```sh
cargo run -- --mode pump --control-flow wait --duration-ms 2200
cargo run -- --mode pump --native-redraw-each-pump --duration-ms 2200
```

Snapshots are written under `artifacts/` when macOS grants Screen Recording
permission to the invoking terminal. Snapshot failure is logged explicitly but
does not replace the navigation, DOM, and JavaScript timer checks.
