// Consts
// const GRID_SIZE: u32 = 1024u;
const GRID_SIZE: u32 = 1024u * 4u;

// Bind Group 0: Storage Buffers (Memory)
// binding(0) is the Previous Frame (Read Only)
// binding(1) is the Current Frame (Write Only)
@group(0) @binding(0) var<storage, read> cellStateIn: array<u32>;
@group(0) @binding(1) var<storage, read_write> cellStateOut: array<u32>;

fn get_index(x: u32, y: u32) -> u32 {
    return (y % GRID_SIZE) * GRID_SIZE + (x % GRID_SIZE);
}

// Compute shader (The Physics)
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;

    if (x >= GRID_SIZE || y >= GRID_SIZE) { return; }

    let index = get_index(x, y);
    
    // Count Neighbors (Toroidal Wrapping)
    var neighbors = 0u;
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            if (i == 0 && j == 0) { continue; }
            let nx = (x + u32(i) + GRID_SIZE) % GRID_SIZE;
            let ny = (y + u32(j) + GRID_SIZE) % GRID_SIZE;
            neighbors += cellStateIn[get_index(nx, ny)];
        }
    }

    let status = cellStateIn[index];

    // Conway's Rules
    if (status == 1u && (neighbors < 2u || neighbors > 3u)) {
        cellStateOut[index] = 0u; // Die
    } else if (status == 0u && neighbors == 3u) {
        cellStateOut[index] = 1u; // Born
    } else {
        cellStateOut[index] = status; // Survive
    }
}

// Vertex shader (The Geometry)
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) v_index: u32) -> VertexOutput {
    // Defines a full-screen triangle to draw on
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0)
    );
    var output: VertexOutput;
    output.position = vec4<f32>(pos[v_index], 0.0, 1.0);
    // Convert Position to UV coordinates for texture mapping
    output.uv = (pos[v_index] + 1.0) * 0.5;
    output.uv.y = 1.0 - output.uv.y; // Flip Y
    return output;
}

// Fragment shader (Visuals)
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Map pixel coordinate on screen to a cell in grid
    let x = u32(in.uv.x * f32(GRID_SIZE));
    let y = u32(in.uv.y * f32(GRID_SIZE));
    let index = get_index(x, y);
    
    let state = cellStateIn[index];
    
    // Colour
    if (state == 1u) {
        // Alive Cell Color
        return vec4<f32>(0.6, 0.2, 1.0, 1.0); // Bright Neon Purple
    } else {
        return vec4<f32>(0.0, 0.0, 0.1, 1.0); // Deep Void Blue
    }
}