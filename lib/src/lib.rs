use winit::event_loop::EventLoop;
#[cfg(target_os = "android")]
use winit::{
    event_loop::EventLoopBuilder, platform::android::activity::AndroidApp,
    platform::android::EventLoopBuilderExtAndroid,
};

mod app;
mod framework;

#[allow(dead_code)]
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    std::env::set_var("RUST_BACKTRACE", "1");
    let event_loop = EventLoopBuilder::new()
        .with_android_app(app)
        .build()
        .unwrap(); //with_android_app
    crate::framework::run::<app::App>(event_loop, "App");
}

#[allow(dead_code)]
fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    let event_loop = EventLoop::new().unwrap(); //with_android_app
    crate::framework::run::<app::App>(event_loop, "App");
}
