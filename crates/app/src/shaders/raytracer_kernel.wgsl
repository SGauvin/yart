struct Material {
    albedo: vec3<f32>,
    is_mirror: u32,
}

struct Sphere {
    center: vec3<f32>,
    radius: f32,
    material: Material,
}

struct Ray {
    direction: vec3<f32>,
    origin: vec3<f32>,
}

struct Camera {
    position: vec3<f32>,
}

struct SceneInfo {
    camera: Camera,
    time: f32,
    sphere_count: u32,
    random_seed: f32,
    frame_count: u32,
}

struct HitResult {
    t: f32,
    point: vec3<f32>,
    normal: vec3<f32>,
    sphere_index: u32,
}

@group(0) @binding(0)
var color_buffer: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(1)
var<uniform> scene_info: SceneInfo;

@group(0) @binding(2)
var<storage, read_write> spheres: array<Sphere>;

@group(0) @binding(3)
var average_colors : texture_2d<f32>;

@group(0) @binding(4)
var screen_sampler : sampler;

var<private> seed: vec2<f32>;

@compute @workgroup_size(1,1,1)
fn main(@builtin(global_invocation_id) GlobalInvocationID : vec3<u32>) {
    let screen_size: vec2<i32> = textureDimensions(color_buffer);
    let screen_pos : vec2<i32> = vec2<i32>(i32(GlobalInvocationID.x), i32(GlobalInvocationID.y));

    seed = vec2<f32>(f32(screen_pos.x) / f32(screen_size.x), f32(screen_pos.y) / f32(screen_size.y)) + scene_info.random_seed;

    var average_color = vec3<f32>(0.0, 0.0, 0.0);
    let sample_count = 100;
    for (var i = 0; i < sample_count; i++) {
        let pixel_color = sample(screen_pos, screen_size);
        average_color += pixel_color / f32(sample_count);
    }

    let pos = vec2<f32>(f32(screen_pos.x) / f32(screen_size.x), f32(screen_pos.y) / f32(screen_size.y));
    let progressive_color: vec3<f32> = textureSampleLevel(average_colors, screen_sampler, pos, 0.0).rgb
        * (f32(scene_info.frame_count - u32(1)) / f32(scene_info.frame_count));
    let final_color = progressive_color + average_color / f32(scene_info.frame_count);

    textureStore(color_buffer, screen_pos, vec4<f32>(final_color, 1.0));
}

fn sample(screen_pos: vec2<i32>, screen_size: vec2<i32>) -> vec3<f32> {
    /* let light_pos = vec3<f32>(10.0, 1.3, -2.0); */
    let forwards = vec3<f32>(1.0, 0.0, 0.0);
    let right = vec3<f32>(0.0, -1.0, 0.0);
    let up = vec3<f32>(0.0, 0.0, 1.0);

    let rand_x = random();
    let rand_y = random();

    let horizontal_coefficient: f32 = (f32(screen_pos.x) + rand_x - f32(screen_size.x) / 2.0) / f32(screen_size.x);
    let vertical_coefficient: f32 = (f32(screen_pos.y) + rand_y - f32(screen_size.y) / 2.0) / f32(screen_size.x);


    var pixel_color = vec3<f32>(1.0, 1.0, 1.0);

    let max_bounces = 150;

    var ray: Ray;
    ray.direction = normalize(forwards + horizontal_coefficient * right + vertical_coefficient * up);
    ray.origin = scene_info.camera.position;

    for (var i = 0; i < max_bounces; i++) {
        var hit_result = hit_any(ray);
        if (hit_result.t > 0.0001) {
            scatter(&ray, &pixel_color, hit_result);
        }
        else {
            // Skybox
            let t = 0.5 * (ray.direction.z + 1.0);
            let skybox_color = (1.0 - t) * vec3<f32>(1.0, 1.0, 1.0) + t * vec3<f32>(0.5, 0.7, 1.0);
            pixel_color *= skybox_color;
            break;
        }
    }
    return pixel_color;
}

fn scatter(ray: ptr<function, Ray>, color: ptr<function, vec3<f32>>, hit_result: HitResult) {
    if (spheres[hit_result.sphere_index].material.is_mirror == u32(1)) {
        (*ray).origin = hit_result.point;
        (*ray).direction = reflect((*ray).direction, hit_result.normal);
        let albedo = spheres[hit_result.sphere_index].material.albedo;
        *color *= albedo;
    }
    else {
        (*ray).origin = hit_result.point;
        let ray_target = (*ray).origin + hit_result.normal + random_on_unit_sphere();
        let direction = ray_target - (*ray).origin;
        if (near_zero(direction)) {
            (*ray).direction = hit_result.normal;
        }
        else {
            (*ray).direction = normalize(direction);
        }
        let albedo = spheres[hit_result.sphere_index].material.albedo;
        *color *= albedo;
    }
}

fn reflect(direction: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    return direction - 2.0 * dot(direction, normal) * normal;
}

fn hit_any(ray: Ray) -> HitResult {
    var min_t: f32 = -1.0;
    var sphere_hit: u32;
    for (var i: u32 = 0u; i < scene_info.sphere_count; i++) {
        let sphere = spheres[i];
        let t: f32 = hit(ray, sphere);
        if (t >= 0.0) {
            if (min_t < 0.0 || t < min_t) {
                min_t = t;
                sphere_hit = i;
            }
        }
    }
    var result: HitResult;
    result.t = min_t;
    result.sphere_index = sphere_hit;
    result.point = ray.origin + ray.direction * min_t;
    result.normal = normalize(result.point - spheres[sphere_hit].center);

    return result;
}

fn hit(ray: Ray, sphere: Sphere) -> f32 {
    let oc = ray.origin - sphere.center;
    let a: f32 = dot(ray.direction, ray.direction);
    let half_b = dot(oc, ray.direction);
    let c = dot(oc, oc) - sphere.radius * sphere.radius;
    let discriminant = half_b * half_b - a * c;
    if (discriminant < 0.0) {
        return -1.0;
    } else {
        return (-half_b - sqrt(discriminant) ) / a;
    }
}

fn near_zero(vec: vec3<f32>) -> bool {
    let s = 1e-7;
    return abs(vec.x) < s && abs(vec.y) < s && abs(vec.z) < s;
}

fn random() -> f32 {
    seed += 0.1;
    return fract(sin(dot(seed.xy, vec2(12.9898,78.233))) * 43758.5453);
}

fn random_in_unit_sphere() -> vec3<f32> {
    while (true) {
        let x = (random() * 2.0) - 1.0;
        let y = (random() * 2.0) - 1.0;
        let z = (random() * 2.0) - 1.0;
        let value = vec3<f32>(x, y, z);
        if (dot(value, value) > 1.0) {
            continue;
        }
        return value;
    }
    return vec3<f32>(0.0, 0.0, 0.0);
}

fn random_on_unit_sphere() -> vec3<f32> {
    return normalize(random_in_unit_sphere());
}
