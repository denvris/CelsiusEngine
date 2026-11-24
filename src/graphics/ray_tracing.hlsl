struct RayObject {
    float4 center_radius; // Positive radius for a sphere, zero for a box.
    float4 extent_material; // Box half extents and the material ID.
    float4 color;
};

cbuffer RayScene : register(b1) {
    RayObject trace_objects[32];
};

Texture2D<float4> concrete_color : register(t2);
Texture2D<float4> concrete_roughness : register(t3);
Texture2D<float4> wood_color : register(t4);
Texture2D<float4> wood_roughness : register(t5);
SamplerState material_sampler : register(s2);

float sphere_distance(float3 p, float3 ray, float4 sphere) {
    float3 delta = p - sphere.xyz;
    float half_b = dot(delta, ray);
    float disc = half_b * half_b - dot(delta, delta) + sphere.w * sphere.w;
    if (disc < 0) return -1;
    float root = sqrt(disc);
    float near_hit = -half_b - root;
    return near_hit > 0.002 ? near_hit : -half_b + root;
}

float box_distance(float3 p, float3 reciprocal, float3 extent) {
    float3 a = (-extent - p) * reciprocal;
    float3 b = ( extent - p) * reciprocal;
    float3 lo = min(a,b);
    float3 hi = max(a,b);
    float entry = max(lo.x, max(lo.y, lo.z));
    float exit = min(hi.x, min(hi.y, hi.z));
    if (entry > exit || exit <= 0.002) return -1;
    return entry > 0.002 ? entry : exit;
}

float3 ray_sky(float3 origin, float3 ray) {
    return clouds_at(origin, ray, environment(ray), 6);
}

float resolution_pattern(float3 world, float footprint) {
    // Five groups with progressively finer vertical line pairs.
    float group_id = clamp(floor((world.x + 3.75) / 1.5), 0, 4);
    float frequency = 3.0 * exp2(group_id);
    float stripe = abs(frac(world.x * frequency) - 0.5);
    float blur = min(0.25, 0.5 * footprint * frequency);
    float edge = smoothstep(0.249 - blur, 0.251 + blur, stripe);
    return lerp(edge, 0.5, saturate(footprint * frequency));
}

float3 diagnostic_color(float3 p, float3 tint, float material, float footprint) {
    if (material == 20) {
        return lerp(float3(0.012, 0.018, 0.028), tint, resolution_pattern(p, footprint));
    }
    return tint;
}

// Shader API. Each call traces at most one bounded scene ray. Misses use HDRI and clouds.
float3 trace_radiance(float3 origin, float3 ray) {
    [branch] if (settings.z < 0.5) return ray_sky(origin, ray);
    float3 direction_sign = float3(ray.x < 0 ? -1 : 1, ray.y < 0 ? -1 : 1, ray.z < 0 ? -1 : 1);
    float3 reciprocal = direction_sign / max(abs(ray), 1e-6);
    float closest = 35;
    int hit_id = -1;
    [loop] for (int i = 0; i < (int)settings.w; ++i) {
        RayObject obj = trace_objects[i];
        float distance_hit = obj.center_radius.w > 0
            ? sphere_distance(origin, ray, obj.center_radius)
            : box_distance(origin - obj.center_radius.xyz, reciprocal, obj.extent_material.xyz);
        if (distance_hit > 0.002 && distance_hit < closest) {
            closest = distance_hit;
            hit_id = i;
        }
    }
    if (hit_id < 0) return ray_sky(origin, ray);

    RayObject hit = trace_objects[hit_id];
    float3 hit_position = origin + closest * ray;
    float material = hit.extent_material.w;
    float3 tint = diagnostic_color(hit_position, hit.color.rgb, material, closest * 0.0015);
    if (material == 20 || material == 2) return tint * 1.18;
    float3 local = hit_position - hit.center_radius.xyz;
    float3 normal = normalize(local);
    if (hit.center_radius.w <= 0) {
        float3 relative = abs(local) / max(hit.extent_material.xyz, 0.0001);
        normal = relative.x >= max(relative.y, relative.z) ? float3(sign(local.x), 0, 0)
            : (relative.y >= relative.z ? float3(0, sign(local.y), 0) : float3(0, 0, sign(local.z)));
    }
    return tint * (0.36 + 0.55 * saturate(dot(normal, sun)));
}
