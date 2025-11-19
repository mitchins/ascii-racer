@group(0) @binding(0)
var color_texture: texture_2d<f32>;

@group(0) @binding(1)
var depth_texture: texture_depth_2d;

@group(0) @binding(2)
var normal_texture: texture_2d<f32>;

@group(0) @binding(3)
var<storage, read_write> ascii_buffer: array<u32>;

const WIDTH: u32 = 160u;
const HEIGHT: u32 = 48u;

// Sobel kernels for edge detection
const sobel_x: array<f32, 9> = array<f32, 9>(
    -1.0, 0.0, 1.0,
    -2.0, 0.0, 2.0,
    -1.0, 0.0, 1.0
);

const sobel_y: array<f32, 9> = array<f32, 9>(
    -1.0, -2.0, -1.0,
     0.0,  0.0,  0.0,
     1.0,  2.0,  1.0
);

fn sample_depth(coord: vec2<i32>) -> f32 {
    let clamped = clamp(coord, vec2<i32>(0, 0), vec2<i32>(i32(WIDTH) - 1, i32(HEIGHT) - 1));
    return textureLoad(depth_texture, clamped, 0);
}

fn sample_color(coord: vec2<i32>) -> vec3<f32> {
    let clamped = clamp(coord, vec2<i32>(0, 0), vec2<i32>(i32(WIDTH) - 1, i32(HEIGHT) - 1));
    return textureLoad(color_texture, clamped, 0).rgb;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let coord = vec2<i32>(i32(global_id.x), i32(global_id.y));
    
    if (global_id.x >= WIDTH || global_id.y >= HEIGHT) {
        return;
    }
    
    // Sample color for luminance
    let color = sample_color(coord);
    let luminance = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    
    // Sobel edge detection on depth
    var edge_x = 0.0;
    var edge_y = 0.0;
    
    for (var ky = -1; ky <= 1; ky++) {
        for (var kx = -1; kx <= 1; kx++) {
            let sample_coord = coord + vec2<i32>(kx, ky);
            let depth = sample_depth(sample_coord);
            
            // Linearize depth for better edge detection
            let linear_depth = 1.0 - depth;
            
            let kernel_idx = (ky + 1) * 3 + (kx + 1);
            edge_x += linear_depth * sobel_x[kernel_idx];
            edge_y += linear_depth * sobel_y[kernel_idx];
        }
    }
    
    let edge_magnitude = sqrt(edge_x * edge_x + edge_y * edge_y);
    
    // Blend luminance with edge detection
    let combined = luminance * 0.7 + edge_magnitude * 10.0;
    
    // Map to ASCII glyph ID (0-68 range for our ASCII ramp)
    let glyph_id = u32(clamp(combined * 68.0, 0.0, 67.0));
    
    let buffer_idx = global_id.y * WIDTH + global_id.x;
    ascii_buffer[buffer_idx] = glyph_id;
}
