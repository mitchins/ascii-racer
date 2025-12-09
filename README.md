# ascii-racer

A terminal-based 3D racing game rendered entirely in ASCII art using headless wgpu.

## Features

- **Headless 3D Rendering**: Uses wgpu for GPU-accelerated 3D rendering without a window
- **160x48 G-Buffer**: Offscreen rendering with color, depth, and normal buffers
- **Edge Detection**: Sobel edge detection on depth buffer for enhanced ASCII art
- **Circular Test Track**: Drive around a 3D circular race track
- **Box Car**: Simple box-shaped car with physics
- **Follow Camera**: Camera automatically follows the car
- **ASCII Art Conversion**: WGSL compute shader converts 3D scene to ASCII characters
- **30 FPS**: Target frame rate with smooth rendering

## Controls

- **Up Arrow**: Accelerate forward
- **Down Arrow**: Reverse/brake
- **Left Arrow**: Steer left
- **Right Arrow**: Steer right
- **Q or Esc**: Quit

## Requirements

- Rust 1.70 or later
- A GPU with Vulkan, Metal, DX12, or OpenGL support
- Terminal with support for ANSI escape codes

## Building and Running

```bash
cargo build --release
cargo run --release
```

## Technical Details

### Rendering Pipeline

1. **3D Scene Rendering**: Renders a circular track and box car to offscreen G-buffer textures
2. **Compute Shader**: Processes G-buffer using Sobel edge detection on depth and luminance calculation
3. **ASCII Mapping**: Maps combined edge and luminance values to 68 ASCII glyphs
4. **Terminal Output**: Displays ASCII buffer to terminal using crossterm in raw mode

### Architecture

- **G-Buffer**: 160x48 resolution with:
  - Color (RGBA8Unorm)
  - Depth (Depth32Float)
  - Normals (RGBA8Snorm)
- **Shaders**:
  - `shader.wgsl`: Vertex and fragment shaders for 3D rendering
  - `compute.wgsl`: Compute shader for ASCII conversion
- **Physics**: Simple velocity-based physics with friction and steering

### ASCII Ramp

Uses 68 characters from darkest to brightest:
```
 .'`^",:;Il!i><~+_-?][}{1)(|\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$
```

## License

Fun… hopefully

