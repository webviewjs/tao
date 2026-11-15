---
"tao": patch
---

Remove macOS run-loop observers when EventLoop is dropped, preventing stale callbacks from panicking when an embedding host continues running AppKit after Tao shuts down.
