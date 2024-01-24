#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

mod app;
mod framework;

#[allow(dead_code)]
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    panic!("changed");
    std::env::set_var("RUST_BACKTRACE", "1");
    crate::framework::run::<app::App>("App");
}

#[allow(dead_code)]
fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    crate::framework::run::<app::App>("App");
}
