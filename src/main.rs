use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{WindowEvent, ElementState, KeyEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
    dpi::PhysicalSize,
    keyboard::{KeyCode, PhysicalKey},
};
use wgpu::util::DeviceExt;
use rayon::prelude::*;

// --- CONFIGURATION ---
// const GRID_SIZE: u32 = 1024;
// Go from 1 Million -> 16 Million cells
const GRID_SIZE: u32 = 4096;
const WORKGROUP_SIZE: u32 = 8;

struct GraphicsState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    compute_pipeline: wgpu::ComputePipeline,
    render_pipeline: wgpu::RenderPipeline,
    bind_group_a: wgpu::BindGroup,
    bind_group_b: wgpu::BindGroup,
    buffer_a: wgpu::Buffer,
    buffer_b: wgpu::Buffer,
    cpu_buffer: Vec<u32>,
    using_cpu: bool,
    step: usize,
}

impl GraphicsState {
    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn compute_cpu(&mut self) {
        let size = GRID_SIZE as usize;
        let input = &self.cpu_buffer;
        
        let next_state: Vec<u32> = (0..input.len()).into_par_iter().map(|index| {
            let x = index % size;
            let y = index / size;
            
            let mut neighbors = 0;
            for i in -1..=1 {
                for j in -1..=1 {
                    if i == 0 && j == 0 { continue; }
                    let nx = (x as i32 + i + size as i32) as usize % size;
                    let ny = (y as i32 + j + size as i32) as usize % size;
                    neighbors += input[ny * size + nx];
                }
            }
            
            let status = input[index];
            if status == 1 && (neighbors < 2 || neighbors > 3) {
                0
            } else if status == 0 && neighbors == 3 {
                1
            } else {
                status
            }
        }).collect();

        self.cpu_buffer = next_state;
    }
}

struct App {
    state: Option<GraphicsState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() { return; }
        let window = Arc::new(event_loop.create_window(
            winit::window::Window::default_attributes().with_title("Initializing...")
        ).unwrap());
        let state = pollster::block_on(init_gpu(window.clone()));
        self.state = Some(state);

        // We must manually request the very first frame to start the loop.
        self.state.as_ref().unwrap().window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(state) = &mut self.state {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(physical_size) => state.resize(physical_size),
                
                WindowEvent::KeyboardInput { event: KeyEvent { state: ElementState::Pressed, physical_key: PhysicalKey::Code(KeyCode::Space), .. }, .. } => {
                    state.using_cpu = !state.using_cpu;
                    println!("Switched to {}", if state.using_cpu { "CPU Mode" } else { "GPU Mode" });
                },

                WindowEvent::RedrawRequested => {
                    let start = Instant::now();

                    // 1. CPU LOGIC (Done FIRST to avoid borrow conflicts)
                    if state.using_cpu {
                        state.compute_cpu();
                        
                        // Upload to GPU
                        let buffer_dest = if state.step % 2 == 0 { &state.buffer_a } else { &state.buffer_b };
                        state.queue.write_buffer(buffer_dest, 0, bytemuck::cast_slice(&state.cpu_buffer));
                    }

                    // 2. NOW we get the GPU resources (Immutable Borrow starts here)
                    let frame = match state.surface.get_current_texture() {
                        Ok(frame) => frame,
                        Err(_) => return,
                    };
                    let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder = state.device.create_command_encoder(&Default::default());

                    // Select Bind Group
                    let bind_group = if state.step % 2 == 0 { &state.bind_group_a } else { &state.bind_group_b };

                    // 3. GPU LOGIC (Only runs if NOT using CPU)
                    if !state.using_cpu {
                        let mut cpass = encoder.begin_compute_pass(&Default::default());
                        cpass.set_pipeline(&state.compute_pipeline);
                        cpass.set_bind_group(0, bind_group, &[]);
                        cpass.dispatch_workgroups(GRID_SIZE / WORKGROUP_SIZE, GRID_SIZE / WORKGROUP_SIZE, 1);
                    }

                    // 4. RENDER PASS (Always runs to show result)
                    {
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: None,
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.3, a: 1.0 }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        rpass.set_pipeline(&state.render_pipeline);
                        rpass.set_bind_group(0, bind_group, &[]);
                        rpass.draw(0..6, 0..1);
                    }

                    state.queue.submit(Some(encoder.finish()));
                    frame.present();
                    state.step += 1;
                    state.window.request_redraw();

                    let duration = start.elapsed();
                    let mode = if state.using_cpu { "CPU (Rayon)" } else { "GPU (WGPU)" };
                    
                    state.window.set_title(&format!(
                        "Rust Life | Mode: {} | Update Time: {:.2?} | {} Cells", 
                        mode, duration, GRID_SIZE * GRID_SIZE
                    ));
                }
                _ => {}
            }
        }
    }
}

async fn init_gpu(window: Arc<Window>) -> GraphicsState {
    let instance = wgpu::Instance::default();
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        ..Default::default()
    }).await.unwrap();
    let (device, queue) = adapter.request_device(&Default::default(), None).await.unwrap();
    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats[0];
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: window.inner_size().width,
        height: window.inner_size().height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    let mut initial_data = vec![0u32; (GRID_SIZE * GRID_SIZE) as usize];
    for i in 0..initial_data.len() {
        if rand::random::<f32>() > 0.8 { initial_data[i] = 1; }
    }

    let buffer_a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Buffer A"),
        contents: bytemuck::cast_slice(&initial_data),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let buffer_b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Buffer B"),
        size: (initial_data.len() * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ],
        label: None,
    });

    let bind_group_a = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: buffer_a.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: buffer_b.as_entire_binding() },
        ],
        label: None,
    });
    let bind_group_b = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: buffer_b.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: buffer_a.as_entire_binding() },
        ],
        label: None,
    });

    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { bind_group_layouts: &[&bind_group_layout], ..Default::default() });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None, layout: Some(&pipeline_layout), module: &shader, entry_point: "main", compilation_options: Default::default(), cache: None,
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None, layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
    });

    GraphicsState {
        window, surface, device, queue, config, compute_pipeline, render_pipeline, bind_group_a, bind_group_b, buffer_a, buffer_b,
        cpu_buffer: initial_data,
        using_cpu: false,
        step: 0,
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { state: None };
    event_loop.run_app(&mut app).unwrap();
}