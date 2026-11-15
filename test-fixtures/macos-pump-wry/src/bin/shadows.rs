//! Run on the main thread: AppKit creation defaults and subsequent style changes.
use tao::{
  event_loop::EventLoop,
  platform::macos::{WindowBuilderExtMacOS, WindowExtMacOS},
  window::WindowBuilder,
};
fn main() {
  let event_loop = EventLoop::new();
  for decorations in [false, true] {
    for resizable in [false, true] {
      for minimizable in [false, true] {
        for shadow in [false, true] {
          let window = WindowBuilder::new()
            .with_visible(false)
            .with_decorations(decorations)
            .with_resizable(resizable)
            .with_minimizable(minimizable)
            .with_has_shadow(shadow)
            .build(&event_loop)
            .unwrap();
          eprintln!("decorations={decorations} resizable={resizable} minimizable={minimizable} requested={shadow} actual={}", window.has_shadow());
          assert_eq!(window.has_shadow(), shadow);
          for enabled in [true, false, true] {
            window.set_decorations(enabled);
            window.set_resizable(enabled);
            window.set_minimizable(enabled);
            assert_eq!(window.has_shadow(), shadow, "style change to {enabled}");
          }
          window.set_has_shadow(!shadow);
          window.set_decorations(false);
          window.set_resizable(false);
          window.set_minimizable(false);
          assert_eq!(window.has_shadow(), !shadow);
        }
      }
    }
  }
}
