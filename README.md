# Rust GPU Life

**A high-performance, parallelized implementation of Conway’s Game of Life utilizing Rust, WGPU, and Compute Shaders.**

This project demonstrates the raw power of General-Purpose GPU (GPGPU) programming by simulating a 4096x4096 grid (approx. 16.7 million cells) in real-time. It features a hot-swappable toggle between a multi-threaded CPU engine (using Rayon) and a massively parallel GPU engine (using WGPU Compute Shaders).

*Zoomed in Visuaization of Conway's Game of Life*
![Demo: Zoom and Pan](media/zoom_fast.gif)

---
## First, What is Conway's Game of Life?

The **Game of Life** is a cellular automaton. It's a discrete model studied in computability theory and complexity science. It is a "zero-player game," meaning its evolution is determined by its initial state, requiring no further input.

The simulation takes place on a grid of cells, each of which is either **Alive (1)** or **Dead (0)**. Every cell interacts with its eight neighbors (horizontal, vertical, and diagonal) according to four simple rules:

1.  **Underpopulation:** A live cell with fewer than 2 neighbors dies.
2.  **Survival:** A live cell with 2 or 3 neighbors lives on to the next generation.
3.  **Overpopulation:** A live cell with more than 3 neighbors dies.
4.  **Reproduction:** A dead cell with exactly 3 live neighbors becomes a live cell.

### Why use it for Benchmarking?
While the rules are simple, they produce complex, chaotic behaviors that are computationally expensive to simulate at scale.
* **Memory Bound:** Every cell update requires reading the state of 8 distinct memory addresses (neighbors).
* **Embarrassingly Parallel:** Since the state of a cell depends only on the *previous* frame, every single cell can be calculated simultaneously.

This makes the Game of Life an ideal candidate for stress-testing **GPU Memory Bandwidth** and **Parallel Compute Architecture**.

---

## Visual Demos
*This project implements two distinct simulation engines to highlight the architectural differences between serial and parallel processors.*

### 1. The CPU Engine (MIMD Architecture)
The CPU implementation uses **Rayon** to execute a **Work-Stealing** parallelism strategy.
* **Logic:** The grid is split into chunks, and the 1D vector of cell states is distributed across available CPU cores (e.g., 8 cores on M3).
* **Bottleneck:** While efficient for complex branching logic, the CPU is bound by the number of physical cores. At 16 million cells, the overhead of memory access and cache misses restricts performance, resulting in linear scaling where simulation time increases directly with grid size.

*Rayon (CPU) visualization. Note the frame-time delta in the window title.*
![Demo: CPU Visualization](media/output_cpu_smart.gif)

### 2. The GPU Engine (SIMT Architecture)
The GPU implementation utilizes **WGPU Compute Shaders** to leverage a "Single Instruction, Multiple Thread" architecture.
* **Massive Parallelism:** Instead of looping, we dispatch thousands of **8x8 Workgroups**. Every cell is updated simultaneously by its own dedicated lightweight thread, mapping the grid directly to the GPU's Global Invocation ID.
* **Zero-Copy Pipeline:** Unlike traditional renderers that copy data between RAM and VRAM, this system uses **Storage Buffers**. The Compute Shader writes the next state to VRAM, and the Fragment Shader reads *directly* from that same buffer to draw the screen.
* **Ping-Pong Buffering:** To prevent race conditions (reading a neighbor that has already been updated), the system maintains two buffers. The compute pass binds `Buffer A` as `read_only` and `Buffer B` as `read_write`, swapping their roles every frame.

*WGPU (Compute Shaders) visualization of the 4096² cells running at 60 FPS. Note the frame-time delta in the window title.*
![Demo: Zoom and Pan](media/output_gpu_smart.gif)

---

## Performance Analysis

The following benchmarks were conducted on an **Apple M3 Pro** (Unified Memory Architecture).

| Metric | Apple M-Series CPU (Parallel) | Apple M-Series GPU (Compute Shader) |
| :--- | :--- | :--- |
| **Time per Frame** | ~30ms | ~5.0ms |
| **FPS** | ~30 FPS | 60 FPS (VSync Capped) |
| **Speedup** | 1x | **6x** |

### Bottleneck Analysis: The "Ferrari in Traffic" Problem
During early testing with smaller grid sizes (1024x1024), the speedup was imperceptible.
* **CPU:** Iterating 1 million integers takes ~2ms.
* **GPU:** Takes ~0.01ms.
* **The Issue:** The monitor refreshes every 16ms (60Hz). Both processors were finishing their work faster than the screen could update.
* **The Solution:** Increased grid size to **16 million cells** (4096x4096) to saturate the CPU, revealing the true performance gap.
*Note: increasing the grid size further would exagerate the difference in compute speed however it would make it impossible to see the vizualization*

---

## Technical Implementation

### 1. The GPU Pipeline (WGSL)
The core simulation runs on a WebGPU pipeline. The architecture follows a strict 6-step process orchestrated by the Rust host:

1.  **Shader Module Creation**: WGSL source is validated via strict static analysis to ensure memory safety.
2.  **Pipeline Creation**: A `GPUComputePipeline` encapsulates the compute state.
3.  **Resource Binding**: Buffers are linked via `@group` and `@binding` attributes, connecting CPU memory definitions to GPU shader variables.
4.  **Execution**: A `dispatch_workgroups` command is issued.
5.  **Invocation**: The GPU executes the shader entry point (`@compute`) in parallel across thousands of threads.
6.  **Output**: Invocations write directly to storage buffers.

### 2. Zero-Copy Architecture
I optimized the pipeline to leverage Unified Memory architectures (like Apple Silicon). The fragment shader reads directly from the Compute Storage Buffers to render the grid, minimizing buffer copy overhead.

### 3. Synchronization Strategy
You may notice that switching from **CPU to GPU** is seamless, but switching **GPU to CPU** reverts the simulation to an old state.
* **Why?** I deliberately chose a unidirectional data flow (CPU to GPU) to maximize bandwidth. The CPU acts as a "state injector," while the GPU runs a free-wheeling simulation.
* **The Trade-off:** Reading the GPU state back to the CPU every frame would require a pipeline stall, killing performance. Therefore, the CPU state remains "frozen" in the past while the GPU simulation advances.

---

## Why Rust?
*Other than me wanting to learn Rust for fun :D*

### 1. Safety in Graphics
`wgpu` provides a safe wrapper around Vulkan/Metal/DX12. It catches validation errors—like the bind group mismatches encountered during development—at compile time or initialization, preventing driver crashes.

### 2. Concurrency
The CPU fallback uses `rayon`. Rust’s borrow checker guarantees that the simulation state cannot be mutated by multiple threads simultaneously without explicit synchronization, allowing for safe parallel iteration.

---

## How to Run

```bash
# Clone the repository
git clone [https://github.com/yourusername/rust_gpu_life.git](https://github.com/yourusername/rust_gpu_life.git)
cd rust_gpu_life

# Run in release mode (Essential for performance benchmarks)
cargo run --release
```

Controls:
* Spacebar: Toggle between CPU and GPU modes.
* Console: Watch standard output for mode switch logs.

---

##  Logbook & Reflections
*A collection of development notes, thoughts, and debugging observations.*

### 1. The "Ferrari in Traffic" Paradox
Early in development, I encountered a counter-intuitive problem: my GPU implementation wasn't visually faster than the CPU version.
* **Observation:** At 1,024 x 1,024 resolution, the CPU calculated frames in ~2ms, while the GPU took ~0.01ms. However, because the monitor is capped at 60Hz (16ms per frame), both implementations looked identical.
* **The Fix:** I had to drastically increase the workload (to 16 million cells) to saturate the CPU.
* **Takeaway:** Performance engineering isn't just about making code fast; it's about understanding the **bottleneck hierarchy**. In this case, the bottleneck was the display hardware, not the compute capability.

### 2. The Cost of Synchronization (Latency vs. Throughput)
I made a deliberate architectural choice to keep the data flow **unidirectional (CPU → GPU)**.
* **The Dilemma:** Switching from GPU mode back to CPU mode results in a "time travel" effect where the simulation reverts to the last CPU state.
* **The Trade-off:** To fix this, I would need to read the GPU buffer back to RAM every frame. This introduces a massive pipeline stall, forcing the CPU to wait for the GPU to finish before proceeding.
* **Decision:** I prioritized **throughput** over state synchronization. In a real-world simulation context, it is rarely efficient to treat the GPU as a co-processor that shares state 1:1 with the CPU; it should be treated as a distinct engine that runs ahead.

### 3. Cellular Entropy & The Second Law
Watching the simulation run for extended periods revealed an interesting behavior akin to the **Second Law of Thermodynamics**.
* **Observation:** A random "soup" of 16 million cells has high entropy. As Conway's rules apply, chaos resolves into order (stable blocks, blinkers, gliders). Eventually, the "temperature" of the system drops until the grid becomes largely static.
* **Curiosity:** This led me to experiment with a "God Mode" shader (not in this release) that randomly injects noise into dead zones, effectively adding energy back into the system to prevent "heat death."

### 4. Why Rust for Graphics?
Graphics programming is notoriously unsafe—one wrong pointer or buffer size and you crash the driver.
* **Experience:** Using `wgpu` felt distinct from my experience with raw OpenGL/Vulkan. The rigorous type system caught synchronization errors (like trying to write to a buffer while it was being read) at compile-time.
* **Conclusion:** Rust didn't just prevent crashes; it acted as a strict mentor, forcing me to understand the lifecycle of my GPU resources before I was allowed to run them.
