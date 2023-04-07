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

@compute @workgroup_size(1,1,1)
fn main(@builtin(global_invocation_id) GlobalInvocationID : vec3<u32>) {

    let screen_size: vec2<i32> = textureDimensions(color_buffer);
    let screen_pos : vec2<i32> = vec2<i32>(i32(GlobalInvocationID.x), i32(GlobalInvocationID.y));

    var seed = vec2<f32>(f32(screen_pos.x) / f32(screen_size.x), f32(screen_pos.y) / f32(screen_size.y)) + scene_info.random_seed;
    var average_color = vec3<f32>(0.0, 0.0, 0.0);
    let sample_count = 10;
    for (var i = 0; i < sample_count; i++) {
        let pixel_color = sample(screen_pos, screen_size, seed);
        average_color += pixel_color / f32(sample_count);
        seed += 0.17;
    }
    textureStore(color_buffer, screen_pos, vec4<f32>(average_color, 1.0));
}

fn sample(screen_pos: vec2<i32>, screen_size: vec2<i32>, seed: vec2<f32>) -> vec3<f32> {
    let light_pos = vec3<f32>(10.0, 1.3, -2.0);
    let forwards = vec3<f32>(1.0, 0.0, 0.0);
    let right = vec3<f32>(0.0, -1.0, 0.0);
    let up = vec3<f32>(0.0, 0.0, 1.0);

    let rand_x = random(seed + 0.1);
    let rand_y = random(seed + 0.2);

    let horizontal_coefficient: f32 = (f32(screen_pos.x) + rand_x - f32(screen_size.x) / 2.0) / f32(screen_size.x); 
    let vertical_coefficient: f32 = (f32(screen_pos.y) + rand_y - f32(screen_size.y) / 2.0) / f32(screen_size.x);


    var pixel_color = vec3<f32>(3.0, 3.0, 3.0);

    let max_bounces = 50;

    var ray: Ray;
    ray.direction = normalize(forwards + horizontal_coefficient * right + vertical_coefficient * up);
    ray.origin = scene_info.camera.position;

    for (var i = 0; i < 3; i++) {
        var hit_result = hit_any(ray);
        if (hit_result.t > 0.0001) {
            // Pixel hit an object
            let hit_pos = hit_result.point;
            let normal = hit_result.normal;

            var new_ray: Ray;
            new_ray.origin = hit_pos;
            let hit_to_light_vec = light_pos - hit_pos;
            new_ray.direction = normalize(hit_to_light_vec);
            let new_hit_result = hit_any(new_ray);

            if (new_hit_result.t < 0.0 || new_hit_result.t * new_hit_result.t > dot(hit_to_light_vec, hit_to_light_vec)) {
                // Not in shadow
                let albedo = spheres[hit_result.sphere_index].material.albedo;
                let light_intensity = dot(normal, new_ray.direction);
                pixel_color *= albedo * light_intensity;
                let ray_target = hit_pos + normal + vec3<f32>(random(seed + 0.11), random(seed + 0.12), random(seed + 0.13));
                ray.origin = hit_pos;
                ray.direction = normalize(ray_target - ray.origin);
            }
            else {
                // Shadow
                pixel_color *= 0.0;
                break;
            }
        }
        else {
            // Skybox
            pixel_color *= vec3<f32>(0.2, 0.4, 0.95);
            break;
        }
    }
    return pixel_color;
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

fn random(co: vec2<f32>) -> f32 {
    return fract(sin(dot(co.xy, vec2(12.9898,78.233))) * 43758.5453);
}
