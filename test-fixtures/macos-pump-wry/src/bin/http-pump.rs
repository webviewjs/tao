//! Localhost reproduction of github/copilot-cli#5398. No input is required.
use std::{
  io::{Read, Write},
  net::TcpListener,
  sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
  },
  thread,
  time::{Duration, Instant},
};
use tao::{
  event::{Event, WindowEvent},
  event_loop::{ControlFlow, EventLoop},
  platform::macos::{EventLoopExtPumpEvents, PumpStatus, WindowExtMacOS},
  window::WindowBuilder,
};
use wry::WebViewBuilder;

fn main() {
  let server = TcpListener::bind("127.0.0.1:0").unwrap();
  let url = format!("http://{}/", server.local_addr().unwrap());
  let roots = Arc::new(AtomicUsize::new(0));
  let ticks = Arc::new(AtomicUsize::new(0));
  let counts = (roots.clone(), ticks.clone());
  thread::spawn(move || {
    for mut stream in server.incoming().flatten() {
      let mut request = [0; 4096];
      let n = stream.read(&mut request).unwrap();
      let tick = String::from_utf8_lossy(&request[..n]).starts_with("GET /tick");
      let body = if tick {
        counts.1.fetch_add(1, Ordering::SeqCst);
        "ok"
      } else {
        counts.0.fetch_add(1, Ordering::SeqCst);
        "<h1>hello</h1><script>setInterval(()=>fetch('/tick'),500);window.ipc.postMessage('ready')</script>"
      };
      write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
    }
  });
  let control_flow = if std::env::args().any(|arg| arg == "--wait") {
    ControlFlow::Wait
  } else {
    ControlFlow::Poll
  };
  let mut event_loop = EventLoop::new();
  event_loop.pump_events(|_, _, flow| *flow = control_flow);
  let ipc = Arc::new(AtomicUsize::new(0));
  let js = Arc::new(AtomicUsize::new(0));
  let mut windows = Vec::new();
  let mut webviews = Vec::new();
  for _ in 0..2 {
    let window = WindowBuilder::new()
      .with_title("HTTP pump regression")
      .build(&event_loop)
      .unwrap();
    let ipc = ipc.clone();
    let webview = WebViewBuilder::new()
      .with_url(&url)
      .with_ipc_handler(move |_| {
        ipc.fetch_add(1, Ordering::SeqCst);
      })
      .build_as_child(&window)
      .unwrap();
    windows.push(window);
    webviews.push(webview);
  }
  let start = Instant::now();
  let mut evaluated = false;
  let mut destroyed = 0;
  let mut input = 0;
  while start.elapsed() < Duration::from_secs(7) {
    assert_eq!(
      event_loop.pump_events(|event, _, flow| {
        *flow = control_flow;
        if matches!(
          event,
          Event::WindowEvent {
            event: WindowEvent::Destroyed,
            ..
          }
        ) {
          destroyed += 1;
        }
      }),
      PumpStatus::Continue
    );
    if !evaluated && start.elapsed() > Duration::from_secs(3) {
      for webview in &webviews {
        let js = js.clone();
        webview
          .evaluate_script_with_callback(
            "document.querySelector('h1').textContent",
            move |result| {
              if result.contains("hello") {
                js.fetch_add(1, Ordering::SeqCst);
              }
            },
          )
          .unwrap();
      }
      evaluated = true;
    }
    // Model the host's 50ms timer, not a workaround in Tao.
    thread::sleep(Duration::from_millis(50));
  }
  assert!(roots.load(Ordering::SeqCst) >= 2);
  assert!(ticks.load(Ordering::SeqCst) >= 4);
  assert_eq!(ipc.load(Ordering::SeqCst), 2);
  assert_eq!(js.load(Ordering::SeqCst), 2);
  // Post keyboard input only after verifying navigation made progress unaided.
  unsafe {
    let window: *mut objc2::runtime::AnyObject = windows[0].ns_window() as _;
    let number: isize = objc2::msg_send![window, windowNumber];
    let characters = objc2_foundation::NSString::from_str("a");
    let event: *mut objc2::runtime::AnyObject = objc2::msg_send![objc2::class!(NSEvent),
      keyEventWithType: 10usize,
      location: objc2_foundation::NSPoint::new(10.0, 10.0),
      modifierFlags: 0usize, timestamp: 0.0f64, windowNumber: number,
      context: std::ptr::null::<objc2::runtime::AnyObject>(),
      characters: &*characters, charactersIgnoringModifiers: &*characters,
      isARepeat: false, keyCode: 0u16];
    assert!(!event.is_null());
    let app: *mut objc2::runtime::AnyObject =
      objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
    let _: () = objc2::msg_send![app, postEvent: event, atStart: false];
  }
  for _ in 0..3 {
    event_loop.pump_events(|event, _, _| {
      if matches!(
        event,
        Event::WindowEvent {
          event: WindowEvent::KeyboardInput { .. },
          ..
        }
      ) {
        input += 1;
      }
    });
  }
  assert!(input > 0, "posted keyboard event must reach Tao");
  drop(webviews);
  drop(windows);
  for _ in 0..3 {
    event_loop.pump_events(|event, _, _| {
      if matches!(
        event,
        Event::WindowEvent {
          event: WindowEvent::Destroyed,
          ..
        }
      ) {
        destroyed += 1;
      }
    });
  }
  let mut shutdown = 0;
  assert_eq!(
    event_loop.pump_events(|event, _, flow| {
      *flow = ControlFlow::Exit;
      if matches!(event, Event::LoopDestroyed) {
        shutdown += 1;
      }
    }),
    PumpStatus::Exit(0)
  );
  eprintln!(
    "roots={} ticks={} ipc={} js={} destroyed={destroyed} shutdown={shutdown}",
    roots.load(Ordering::SeqCst),
    ticks.load(Ordering::SeqCst),
    ipc.load(Ordering::SeqCst),
    js.load(Ordering::SeqCst)
  );
  assert!(roots.load(Ordering::SeqCst) >= 2);
  assert!(ticks.load(Ordering::SeqCst) >= 4);
  assert_eq!(ipc.load(Ordering::SeqCst), 2);
  assert_eq!(js.load(Ordering::SeqCst), 2);
  assert_eq!(destroyed, 2);
  assert_eq!(shutdown, 1);
  drop(event_loop);
  // An embedding host continues running CF sources after Tao has shut down.
  unsafe {
    let run_loop: *mut objc2::runtime::AnyObject =
      objc2::msg_send![objc2::class!(NSRunLoop), currentRunLoop];
    let date: *mut objc2::runtime::AnyObject =
      objc2::msg_send![objc2::class!(NSDate), dateWithTimeIntervalSinceNow: 0.01f64];
    let _: () = objc2::msg_send![run_loop, runUntilDate: date];
  }
}
