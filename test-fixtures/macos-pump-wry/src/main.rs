use std::{
  env,
  path::{Path, PathBuf},
  process::Command,
  sync::{Arc, Mutex},
  thread,
  time::{Duration, Instant},
};

use image::GenericImageView;
use objc2::{msg_send, runtime::AnyObject};
use tao::{
  dpi::LogicalSize,
  event::{Event, WindowEvent},
  event_loop::{ControlFlow, EventLoop},
  platform::macos::{EventLoopExtPumpEvents, PumpStatus, WindowExtMacOS},
  platform::run_return::EventLoopExtRunReturn,
  window::{Window, WindowBuilder},
};
use wry::{PageLoadEvent, WebView, WebViewBuilder};

const HTML: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>pump-fixture</title>
<style>html,body{margin:0;width:100%;height:100%;background:#ff00ff;color:white;font:700 48px system-ui;display:grid;place-items:center}#status{text-align:center}</style>
</head><body><div id="status">MAGENTA FRAME<br><span id="counter">0</span></div>
<script>window.fixtureTicks=0;setInterval(()=>{window.fixtureTicks++;document.querySelector('#counter').textContent=window.fixtureTicks},100);</script>
</body></html>"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
  Pump,
  Blocking,
}

struct Options {
  mode: Mode,
  duration: Duration,
  output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let options = parse_options()?;
  std::fs::create_dir_all(&options.output)?;
  let started = Instant::now();
  log(started, 0, format_args!("construct event loop mode={:?}", options.mode));
  let mut event_loop = EventLoop::<()>::new();
  let window = WindowBuilder::new()
    .with_title(format!("Tao Wry {:?}", options.mode))
    .with_inner_size(LogicalSize::new(800.0, 600.0))
    .with_visible(true)
    .build(&event_loop)?;
  let window_number = native_window_number(&window);
  log(
    started,
    0,
    format_args!("window created number={window_number} visible=true"),
  );

  let navigation = Arc::new(Mutex::new(Vec::new()));
  let navigation_log = Arc::clone(&navigation);
  let webview = WebViewBuilder::new()
    .with_html(HTML)
    .with_on_page_load_handler(move |event, url| {
      let event = match event {
        PageLoadEvent::Started => "started",
        PageLoadEvent::Finished => "finished",
      };
      navigation_log.lock().unwrap().push((event, url.clone()));
      eprintln!("[fixture][navigation] event={event} url={url}");
    })
    .build_as_child(&window)?;
  log(started, 0, format_args!("WKWebView attached"));

  match options.mode {
    Mode::Pump => run_pump(
      &mut event_loop,
      &window,
      &webview,
      window_number,
      &options,
      started,
    )?,
    Mode::Blocking => run_blocking(
      event_loop,
      window,
      webview,
      window_number,
      &options,
      started,
    ),
  }

  let finished = navigation
    .lock()
    .unwrap()
    .iter()
    .any(|(event, _)| *event == "finished");
  log(started, 0, format_args!("navigation_finished={finished}"));
  Ok(())
}

fn run_pump(
  event_loop: &mut EventLoop<()>,
  window: &Window,
  webview: &WebView,
  window_number: i64,
  options: &Options,
  started: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
  let mut pump = 0_u64;
  let mut probed = false;
  let mut mutated = false;
  let mut captured = false;
  while started.elapsed() < options.duration {
    pump += 1;
    let status = event_loop.pump_events(|event, _, control_flow| {
      *control_flow = ControlFlow::Poll;
      log(started, pump, format_args!("event={event:?}"));
    });
    log(
      started,
      pump,
      format_args!("pump returned status={status:?} native={}", native_state(window)),
    );
    if matches!(status, PumpStatus::Exit(_)) {
      break;
    }

    let elapsed = started.elapsed();
    if elapsed >= Duration::from_millis(500) && !probed {
      probe_javascript(webview, started, pump, "initial");
      probed = true;
    }
    if elapsed >= Duration::from_secs(1) && !mutated {
      webview.evaluate_script(
        "document.body.style.background='#00bfff';document.querySelector('#status').firstChild.textContent='CYAN MUTATION';",
      )?;
      probe_javascript(webview, started, pump, "mutated");
      mutated = true;
    }
    if elapsed >= Duration::from_millis(1500) && !captured {
      capture_and_report(window_number, &options.output.join("pump-before-resize.png"), started, pump);
      captured = true;
    }
    thread::sleep(Duration::from_millis(16));
  }
  if !captured {
    capture_and_report(window_number, &options.output.join("pump-before-resize.png"), started, pump);
  }
  Ok(())
}

fn run_blocking(
  mut event_loop: EventLoop<()>,
  window: Window,
  webview: WebView,
  window_number: i64,
  options: &Options,
  started: Instant,
) -> ! {
  let deadline = started + options.duration;
  let mut probed = false;
  let mut mutated = false;
  let mut captured = false;
  event_loop.run_return(move |event, _, control_flow| {
    *control_flow = ControlFlow::WaitUntil(deadline);
    log(started, 0, format_args!("event={event:?}"));
    if started.elapsed() >= Duration::from_millis(500) && !probed {
      probe_javascript(&webview, started, 0, "initial");
      probed = true;
    }
    if started.elapsed() >= Duration::from_secs(1) && !mutated {
      let _ = webview.evaluate_script(
        "document.body.style.background='#00bfff';document.querySelector('#status').firstChild.textContent='CYAN MUTATION';",
      );
      probe_javascript(&webview, started, 0, "mutated");
      mutated = true;
    }
    if started.elapsed() >= Duration::from_millis(1500) && !captured {
      capture_and_report(window_number, &options.output.join("blocking.png"), started, 0);
      captured = true;
    }
    if Instant::now() >= deadline
      || matches!(event, Event::WindowEvent { event: WindowEvent::CloseRequested, .. })
    {
      log(started, 0, format_args!("blocking exit native={}", native_state(&window)));
      *control_flow = ControlFlow::Exit;
    }
  });
  std::process::exit(0)
}

fn probe_javascript(webview: &WebView, started: Instant, pump: u64, label: &'static str) {
  let script = "JSON.stringify({ready:document.readyState,title:document.title,text:document.body.innerText,ticks:window.fixtureTicks,bg:getComputedStyle(document.body).backgroundColor})";
  if let Err(error) = webview.evaluate_script_with_callback(script, move |value| {
    log(started, pump, format_args!("js[{label}]={value}"));
  }) {
    log(started, pump, format_args!("js[{label}] error={error}"));
  }
}

fn capture_and_report(window_number: i64, path: &Path, started: Instant, pump: u64) {
  let status = Command::new("screencapture")
    .args(["-x", "-l", &window_number.to_string()])
    .arg(path)
    .status();
  match status {
    Ok(status) if status.success() => match image::open(path) {
      Ok(image) => {
        let (width, height) = image.dimensions();
        let mut sum = [0_u64; 3];
        let mut magenta = 0_u64;
        let mut cyan = 0_u64;
        for pixel in image.to_rgb8().pixels() {
          sum[0] += u64::from(pixel[0]);
          sum[1] += u64::from(pixel[1]);
          sum[2] += u64::from(pixel[2]);
          magenta += u64::from(pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220);
          cyan += u64::from(pixel[0] < 40 && pixel[1] > 150 && pixel[2] > 200);
        }
        let count = u64::from(width) * u64::from(height);
        log(
          started,
          pump,
          format_args!(
            "snapshot={} dimensions={}x{} mean={:.1},{:.1},{:.1} magenta={:.2}% cyan={:.2}%",
            path.display(), width, height,
            sum[0] as f64 / count as f64, sum[1] as f64 / count as f64, sum[2] as f64 / count as f64,
            magenta as f64 * 100.0 / count as f64, cyan as f64 * 100.0 / count as f64,
          ),
        );
      }
      Err(error) => log(started, pump, format_args!("snapshot decode failed: {error}")),
    },
    Ok(status) => log(started, pump, format_args!("screencapture exited {status}")),
    Err(error) => log(started, pump, format_args!("screencapture failed: {error}")),
  }
}

fn native_window_number(window: &Window) -> i64 {
  unsafe { msg_send![window.ns_window() as *mut AnyObject, windowNumber] }
}

fn native_state(window: &Window) -> String {
  unsafe {
    let ns_window = window.ns_window() as *mut AnyObject;
    let visible: bool = msg_send![ns_window, isVisible];
    let key: bool = msg_send![ns_window, isKeyWindow];
    format!("visible={visible} key={key}")
  }
}

fn log(started: Instant, pump: u64, message: std::fmt::Arguments<'_>) {
  eprintln!(
    "[{:.3}ms][pump={pump}][{:?}] {message}",
    started.elapsed().as_secs_f64() * 1000.0,
    thread::current().id(),
  );
}

fn parse_options() -> Result<Options, Box<dyn std::error::Error>> {
  let mut mode = None;
  let mut duration = Duration::from_secs(3);
  let mut output = PathBuf::from("artifacts");
  let mut args = env::args().skip(1);
  while let Some(arg) = args.next() {
    match arg.as_str() {
      "--mode" => {
        mode = Some(match args.next().as_deref() {
          Some("pump") => Mode::Pump,
          Some("blocking") => Mode::Blocking,
          value => return Err(format!("invalid --mode {value:?}").into()),
        });
      }
      "--duration-ms" => duration = Duration::from_millis(args.next().ok_or("missing duration")?.parse()?),
      "--output" => output = PathBuf::from(args.next().ok_or("missing output")?),
      _ => return Err(format!("unknown argument {arg}").into()),
    }
  }
  Ok(Options {
    mode: mode.ok_or("--mode pump|blocking is required")?,
    duration,
    output,
  })
}
