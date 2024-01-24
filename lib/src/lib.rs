#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

mod app;
mod framework;

#[allow(dead_code)]
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    use log::LevelFilter;
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Trace));
    crate::framework::run::<app::App>("App");
}

#[allow(dead_code)]
fn main() {
    crate::framework::run::<app::App>("App");
}
