use std::sync::Arc;

use wgpu::{Instance, Surface};
use winit::{
    dpi::PhysicalSize,
    event::{Event, KeyEvent, StartCause, WindowEvent},
    event_loop::{EventLoop, EventLoopWindowTarget},
    keyboard::{Key, NamedKey},
    window::Window,
};

pub trait Example: 'static + Sized {
    const SRGB: bool = true;

    fn optional_features() -> wgpu::Features {
        wgpu::Features::empty()
    }

    fn required_features() -> wgpu::Features {
        wgpu::Features::empty()
    }

    fn required_downlevel_capabilities() -> wgpu::DownlevelCapabilities {
        wgpu::DownlevelCapabilities {
            flags: wgpu::DownlevelFlags::empty(),
            shader_model: wgpu::ShaderModel::Sm5,
            ..wgpu::DownlevelCapabilities::default()
        }
    }

    fn required_limits() -> wgpu::Limits {
        wgpu::Limits::downlevel_webgl2_defaults() // These downlevel limits will allow the code to run on all possible hardware
    }

    fn init(
        config: &wgpu::SurfaceConfiguration,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self;

    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    );

    fn window_event(&mut self, event: WindowEvent);

    fn render(&mut self, view: &wgpu::TextureView, device: &wgpu::Device, queue: &wgpu::Queue);
}

pub fn create_window(event_loop: &EventLoop<()>, title: &str) -> Arc<Window> {
    let mut builder = winit::window::WindowBuilder::new();
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowBuilderExtWebSys;
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        builder = builder.with_canvas(Some(canvas));
    }
    builder = builder
        .with_title(title)
        .with_inner_size(winit::dpi::PhysicalSize::new(720, 1280));
    Arc::new(builder.build(&event_loop).unwrap())
}

/// Wrapper type which manages the surface and surface configuration.
///
/// As surface usage varies per platform, wrapping this up cleans up the event loop code.
struct SurfaceWrapper {
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
}

impl SurfaceWrapper {
    /// Create a new surface wrapper with no surface or configuration.
    fn new() -> Self {
        Self {
            surface: None,
            config: None,
        }
    }

    /// Called after the instance is created, but before we request an adapter.
    ///
    /// On wasm, we need to create the surface here, as the WebGL backend needs
    /// a surface (and hence a canvas) to be present to create the adapter.
    ///
    /// We cannot unconditionally create a surface here, as Android requires
    /// us to wait until we recieve the `Resumed` event to do so.
    fn pre_adapter(&mut self, instance: &Instance, window: Arc<Window>) {
        if cfg!(target_arch = "wasm32") {
            self.surface = Some(instance.create_surface(window).unwrap());
        }
    }

    /// Check if the event is the start condition for the surface.
    fn start_condition(e: &Event<()>) -> bool {
        match e {
            // On all other platforms, we can create the surface immediately.
            Event::NewEvents(StartCause::Init) => !cfg!(target_os = "android"),
            // On android we need to wait for a resumed event to create the surface.
            Event::Resumed => cfg!(target_os = "android"),
            _ => false,
        }
    }

    /// Called when an event which matches [`Self::start_condition`] is recieved.
    ///
    /// On all native platforms, this is where we create the surface.
    ///
    /// Additionally, we configure the surface based on the (now valid) window size.
    fn resume(&mut self, context: &ExampleContext, window: Arc<Window>, srgb: bool) {
        // Window size is only actually valid after we enter the event loop.
        let window_size = window.inner_size();
        let width = window_size.width.max(1);
        let height = window_size.height.max(1);

        log::info!("Surface resume {window_size:?}");

        // We didn't create the surface in pre_adapter, so we need to do so now.
        if !cfg!(target_arch = "wasm32") {
            self.surface = Some(context.instance.create_surface(window).unwrap());
        }

        // From here on, self.surface should be Some.

        let surface = self.surface.as_ref().unwrap();

        // Get the default configuration,
        let mut config = surface
            .get_default_config(&context.adapter, width, height)
            .expect("Surface isn't supported by the adapter.");
        if srgb {
            // Not all platforms (WebGPU) support sRGB swapchains, so we need to use view formats
            let view_format = config.format.add_srgb_suffix();
            config.view_formats.push(view_format);
        } else {
            // All platforms support non-sRGB swapchains, so we can just use the format directly.
            let format = config.format.remove_srgb_suffix();
            config.format = format;
            config.view_formats.push(format);
        };

        surface.configure(&context.device, &config);
        self.config = Some(config);
    }

    /// Resize the surface, making sure to not resize to zero.
    fn resize(&mut self, context: &ExampleContext, size: PhysicalSize<u32>) {
        log::info!("Surface resize {size:?}");

        let config = self.config.as_mut().unwrap();
        config.width = size.width.max(1);
        config.height = size.height.max(1);
        let surface = self.surface.as_ref().unwrap();
        surface.configure(&context.device, config);
    }

    /// Acquire the next surface texture.
    fn acquire(&mut self, context: &ExampleContext) -> wgpu::SurfaceTexture {
        let surface = self.surface.as_ref().unwrap();

        match surface.get_current_texture() {
            Ok(frame) => frame,
            // If we timed out, just try again
            Err(wgpu::SurfaceError::Timeout) => surface
                .get_current_texture()
                .expect("Failed to acquire next surface texture!"),
            Err(
                // If the surface is outdated, or was lost, reconfigure it.
                wgpu::SurfaceError::Outdated
                | wgpu::SurfaceError::Lost
                // If OutOfMemory happens, reconfiguring may not help, but we might as well try
                | wgpu::SurfaceError::OutOfMemory,
            ) => {
                surface.configure(&context.device, self.config());
                surface
                    .get_current_texture()
                    .expect("Failed to acquire next surface texture!")
            }
        }
    }

    /// On suspend on android, we drop the surface, as it's no longer valid.
    ///
    /// A suspend event is always followed by at least one resume event.
    fn suspend(&mut self) {
        if cfg!(target_os = "android") {
            self.surface = None;
        }
    }

    fn get(&self) -> Option<&Surface> {
        self.surface.as_ref()
    }

    fn config(&self) -> &wgpu::SurfaceConfiguration {
        self.config.as_ref().unwrap()
    }
}

/// Context containing global wgpu resources.
struct ExampleContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}
impl ExampleContext {
    /// Initializes the example context.
    async fn init_async<E: Example>(surface: &mut SurfaceWrapper, window: Arc<Window>) -> Self {
        log::info!("Initializing wgpu...");

        let backends = wgpu::util::backend_bits_from_env().unwrap_or_default();
        let dx12_shader_compiler = wgpu::util::dx12_shader_compiler_from_env().unwrap_or_default();
        let gles_minor_version = wgpu::util::gles_minor_version_from_env().unwrap_or_default();

        let flags = wgpu::InstanceFlags::from_build_config().with_env();
        log::info!("InstanceFlags: {:?}", flags);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags,
            dx12_shader_compiler,
            gles_minor_version,
        });
        surface.pre_adapter(&instance, window);
        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, surface.get())
            .await
            .expect("No suitable GPU adapters found on the system!");

        let adapter_info = adapter.get_info();
        log::info!("Using {} ({:?})", adapter_info.name, adapter_info.backend);

        let optional_features = E::optional_features();
        let required_features = E::required_features();
        let adapter_features = adapter.features();
        assert!(
            adapter_features.contains(required_features),
            "Adapter does not support required features for this example: {:?}",
            required_features - adapter_features
        );

        let required_downlevel_capabilities = E::required_downlevel_capabilities();
        let downlevel_capabilities = adapter.get_downlevel_capabilities();
        assert!(
            downlevel_capabilities.shader_model >= required_downlevel_capabilities.shader_model,
            "Adapter does not support the minimum shader model required to run this example: {:?}",
            required_downlevel_capabilities.shader_model
        );
        assert!(
            downlevel_capabilities
                .flags
                .contains(required_downlevel_capabilities.flags),
            "Adapter does not support the downlevel capabilities required to run this example: {:?}",
            required_downlevel_capabilities.flags - downlevel_capabilities.flags
        );

        // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the surface.
        let needed_limits = E::required_limits().using_resolution(adapter.limits());

        let trace_dir = std::env::var("WGPU_TRACE");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: (optional_features & adapter_features) | required_features,
                    required_limits: needed_limits,
                },
                trace_dir.ok().as_ref().map(std::path::Path::new),
            )
            .await
            .expect("Unable to find a suitable GPU adapter!");

        Self {
            instance,
            adapter,
            device,
            queue,
        }
    }
}

struct FrameCounter {
    // Instant of the last time we printed the frame time.
    last_printed_instant: web_time::Instant,
    // Number of frames since the last time we printed the frame time.
    frame_count: u32,
}

impl FrameCounter {
    fn new() -> Self {
        Self {
            last_printed_instant: web_time::Instant::now(),
            frame_count: 0,
        }
    }

    fn update(&mut self) {
        self.frame_count += 1;
        let new_instant = web_time::Instant::now();
        let elapsed_secs = (new_instant - self.last_printed_instant).as_secs_f32();
        if elapsed_secs > 1.0 {
            let elapsed_ms = elapsed_secs * 1000.0;
            let frame_time = elapsed_ms / self.frame_count as f32;
            let fps = self.frame_count as f32 / elapsed_secs;
            log::info!("Frame time {:.2}ms ({:.1} FPS)", frame_time, fps);

            self.last_printed_instant = new_instant;
            self.frame_count = 0;
        }
    }
}

async fn start<E: Example>(event_loop: EventLoop<()>, title: &str) {
    let window = create_window(&event_loop, title);
    let mut surface = SurfaceWrapper::new();
    let context = ExampleContext::init_async::<E>(&mut surface, window.clone()).await;
    let mut frame_counter = FrameCounter::new();

    // We wait to create the example until we have a valid surface.
    let mut example = None;

    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            use winit::platform::web::EventLoopExtWebSys;
            let event_loop_function = EventLoop::spawn;
        } else {
            let event_loop_function = EventLoop::run;
        }
    }

    log::info!("Entering event loop...");
    // On native this is a result, but on wasm it's a unit type.
    #[allow(clippy::let_unit_value)]
    let _ = (event_loop_function)(
        event_loop,
        move |event: Event<()>, target: &EventLoopWindowTarget<()>| {
            match event {
                ref e if SurfaceWrapper::start_condition(e) => {
                    surface.resume(&context, window.clone(), E::SRGB);

                    // If we haven't created the example yet, do so now.
                    if example.is_none() {
                        example = Some(E::init(
                            surface.config(),
                            &context.adapter,
                            &context.device,
                            &context.queue,
                        ));
                    }
                }
                Event::Suspended => {
                    surface.suspend();
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::Resized(size) => {
                        surface.resize(&context, size);
                        example.as_mut().unwrap().resize(
                            surface.config(),
                            &context.device,
                            &context.queue,
                        );

                        window.request_redraw();
                    }
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                logical_key: Key::Named(NamedKey::Escape),
                                ..
                            },
                        ..
                    }
                    | WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                logical_key: Key::Character(s),
                                ..
                            },
                        ..
                    } if s == "r" => {
                        println!("{:#?}", context.instance.generate_report());
                    }
                    WindowEvent::RedrawRequested => {
                        // On MacOS, currently redraw requested comes in _before_ Init does.
                        // If this happens, just drop the requested redraw on the floor.
                        //
                        // See https://github.com/rust-windowing/winit/issues/3235 for some discussion
                        if example.is_none() {
                            return;
                        }

                        frame_counter.update();

                        let frame = surface.acquire(&context);
                        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
                            format: Some(surface.config().view_formats[0]),
                            ..wgpu::TextureViewDescriptor::default()
                        });

                        example
                            .as_mut()
                            .unwrap()
                            .render(&view, &context.device, &context.queue);

                        window.pre_present_notify();
                        frame.present();
                        window.request_redraw();
                    }
                    _ => example.as_mut().unwrap().window_event(event),
                },
                _ => {}
            }
        },
    );
}

pub fn run<E: Example>(event_loop: EventLoop<()>, title: &'static str) {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            wasm_bindgen_futures::spawn_local(async move { start::<E>(event_loop, title).await })
        } else {
            pollster::block_on(start::<E>(event_loop, title));
        }
    }
}

#[cfg(target_arch = "wasm32")]
/// Parse the query string as returned by `web_sys::window()?.location().search()?` and get a
/// specific key out of it.
pub fn parse_url_query_string<'a>(query: &'a str, search_key: &str) -> Option<&'a str> {
    let query_string = query.strip_prefix('?')?;

    for pair in query_string.split('&') {
        let mut pair = pair.split('=');
        let key = pair.next()?;
        let value = pair.next()?;

        if key == search_key {
            return Some(value);
        }
    }

    None
}
