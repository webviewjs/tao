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

## Localhost, IPC and host lifecycle regression

```sh
cargo run --bin http-pump
cargo run --bin http-pump -- --wait
```

These main-thread integration checks adapt the localhost reproduction from
https://github.com/github/copilot-cli/issues/5398. They launch Tao first, create
two windows, load HTTP pages, require recurring fetch requests, evaluate JS in
both windows and receive Wry IPC. Only after these checks succeed do they post
keyboard input and assert its delivery, destroy both windows, request shutdown
and continue the host's NSRunLoop after dropping Tao. The 50ms delay models the
external host timer; it is not part of Tao's pump implementation. Run with an
external timeout (for example 30 seconds) to catch a stuck pump.

On the unmodified merge `392ff566`, HTTP, JS, IPC, destruction and shutdown
passed on macOS 27.0.1. Continuing NSRunLoop after dropping Tao reproduced both
reported panics (`The panic info must exist here` followed by the panic handler's
`Option::unwrap()`). The run-loop observers remained registered after their
PanicInfo owner was destroyed. The lifecycle fix removes and releases those
observers with EventLoop and balances their weak context references.

The reported blank-page failure did **not** reproduce on this host. Repeated
NSApplication.run(), stop conditions and the one-boundary source allowance
remain unchanged; there is insufficient evidence here to replace them. The
loading behavior still needs testing on macOS 26 with the affected Node addon;
this Rust fixture does not validate libuv/N-API integration.

## Shadow regression

```sh
cargo run --bin shadows
```

Checks all 16 combinations of decorations, resizable, minimizable and shadow,
then toggles style flags and the explicit shadow value. On macOS 27.0.1 the
unmodified merge failed the reported all-false-style/true-shadow combination.
Explicitly applying both shadow values passes the matrix. AppKit preserved the
explicit value during the tested style mutations, so additional stored shadow
state or setStyleMask overrides were unnecessary. This checks hasShadow state;
it does not visually measure the composited shadow.

Neither macOS 15 nor macOS 26 was available locally. Run both fixtures on those
versions before claiming the original OS-specific reports are resolved.
