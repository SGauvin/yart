struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texcoords: vec2<f32>,
};

struct Uniforms {
    time: f32,
};
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @builtin(vertex_index) in_vertex_index: u32,
) -> VertexOutput {
    var vertices = array<vec2<f32>,3>(vec2<f32>(-1.,-1.), vec2<f32>(3.,-1.), vec2<f32>(-1., 3.));
    var out: VertexOutput;
    out.clip_position = vec4<f32>(vertices[in_vertex_index], 0.0, 1.0);
    out.texcoords = 0.5 * out.clip_position.xy + vec2<f32>(0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.texcoords;
    let col = 0.5 + 0.5*cos((uniforms.time / 30.0) + uv.xyx+vec3<f32>(0.,2.,4.));
    return vec4<f32>(col, 1.);
}
