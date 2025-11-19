struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
}

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    let world_position = uniforms.model * vec4<f32>(vertex.position, 1.0);
    out.clip_position = uniforms.view_proj * world_position;
    out.world_position = world_position.xyz;
    
    // Transform normal to world space
    out.world_normal = normalize((uniforms.model * vec4<f32>(vertex.normal, 0.0)).xyz);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    
    // Simple lighting
    let light_dir = normalize(vec3<f32>(1.0, 2.0, 1.0));
    let ambient = 0.3;
    let diffuse = max(dot(in.world_normal, light_dir), 0.0);
    let brightness = ambient + diffuse * 0.7;
    
    out.color = vec4<f32>(brightness, brightness, brightness, 1.0);
    out.normal = vec4<f32>(in.world_normal * 0.5 + 0.5, 1.0);
    
    return out;
}
