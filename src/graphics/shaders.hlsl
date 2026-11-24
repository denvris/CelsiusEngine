cbuffer Eye : register(b0) {
    float4x4 view_projection;
    float4x4 inverse_view_projection;
    float4x4 light_view_projection;
    float4 camera;
    float4 settings;
    float4 material_maps;
};

Texture2D<float4> environment_map : register(t0);
SamplerState environment_sampler : register(s0);
Texture2D<float> shadow_map : register(t1);
SamplerComparisonState shadow_sampler : register(s1);
static const float3 sun = float3(-0.3578, 0.7155, -0.6000);

float3 tonemap(float3 v) {
    v = max(v, 0);
    return saturate(v * (2.51 * v + 0.03) / (v * (2.43 * v + 0.59) + 0.14));
}

float3 environment(float3 ray) {
    if (settings.x > 0.5) {
        float2 uv = float2(atan2(ray.x, -ray.z) * 0.15915494 + 0.5,
                          acos(clamp(ray.y, -1, 1)) * 0.31830989);
        return environment_map.SampleLevel(environment_sampler, uv, 0).rgb;
    }
    float3 sky = lerp(float3(0.43, 0.61, 0.92), float3(0.035, 0.15, 0.45), sqrt(saturate(ray.y)));
    float sun_dot = saturate(dot(ray, sun));
    return sky + float3(4.0, 2.4, 1.1) * (pow(sun_dot, 180) * 0.24 + smoothstep(0.9991, 0.9995, sun_dot));
}

float hash(float3 p) {
    p = frac(p * 0.3183099 + 0.1) * 17;
    return frac(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float noise(float3 p) {
    float3 i = floor(p), f = frac(p);
    f = f * f * (3 - 2 * f);
    float a = lerp(hash(i), hash(i + float3(1,0,0)), f.x);
    float b = lerp(hash(i + float3(0,1,0)), hash(i + float3(1,1,0)), f.x);
    float c = lerp(hash(i + float3(0,0,1)), hash(i + float3(1,0,1)), f.x);
    float d = lerp(hash(i + float3(0,1,1)), hash(i + 1), f.x);
    return lerp(lerp(a, b, f.y), lerp(c, d, f.y), f.z);
}

float3 clouds_at(float3 origin, float3 ray, float3 sky, int steps) {
    if (ray.y <= 0.035 || settings.y <= 0) return sky;
    const float floor_height = 24;
    const float ceiling_height = 40;
    float start = max((floor_height - origin.y) / ray.y, 0);
    float end = min((ceiling_height - origin.y) / ray.y, 280);
    if (start >= end) return sky;
    float stride = (end - start) / steps;
    float transmittance = 1;
    float3 light = 0;
    [loop] for (int i = 0; i < steps && transmittance > 0.025; ++i) {
        float3 p = origin + ray * (start + (i + 0.5) * stride);
        float height = saturate((p.y - floor_height) / (ceiling_height - floor_height));
        float profile = smoothstep(0, 0.25, height) * (1 - smoothstep(0.67, 1, height));
        float3 q = p * 0.085;
        float shape = noise(q) * 0.72 + noise(q * 2.07) * 0.28;
        float density = saturate((shape - 0.43) * 2.7) * profile * settings.y;
        float opacity = 1 - exp(-density * stride * 0.32);
        light += transmittance * opacity * lerp(float3(0.46,0.57,0.77), float3(1.55,1.45,1.28), height);
        transmittance *= 1 - opacity;
    }
    return lerp(sky, light + sky * transmittance, smoothstep(0.035, 0.18, ray.y));
}

struct SkyVertex {
    float4 position : SV_Position;
    float2 clip : TEXCOORD0;
};

SkyVertex sky_vs(uint id : SV_VertexID) {
    SkyVertex o;
    o.clip = float2(id == 2 ? 3 : -1, id == 1 ? 3 : -1);
    o.position = float4(o.clip, 1, 1);
    return o;
}

float4 sky_ps(SkyVertex i) : SV_Target {
    float4 world = mul(inverse_view_projection, float4(i.clip, 1, 1));
    float3 ray = normalize(world.xyz / world.w - camera.xyz);
    return float4(tonemap(clouds_at(camera.xyz, ray, environment(ray), 12)), 1);
}

struct Vertex {
    float3 position : POSITION;
    float3 normal : NORMAL;
    float3 color : COLOR;
    float material : TEXCOORD0;
    float4 sphere : TEXCOORD1;
};

struct Surface {
    float4 position : SV_Position;
    float3 world : TEXCOORD0;
    float3 normal : TEXCOORD1;
    float3 color : COLOR;
    nointerpolation float material : TEXCOORD2;
    nointerpolation float4 sphere : TEXCOORD3;
};

Surface mesh_vs(Vertex v) {
    Surface o;
    o.position = mul(view_projection, float4(v.position, 1));
    o.world = v.position;
    o.normal = v.normal;
    o.color = v.color;
    o.material = v.material;
    o.sphere = v.sphere;
    return o;
}

float3 shade_surface(Surface surface);

float sunlight_visibility(float3 world, float3 normal) {
    float4 clip = mul(light_view_projection, float4(world + normal * 0.009, 1));
    float2 uv = clip.xy * float2(0.5, -0.5) + 0.5;
    if (any(uv < 0) || any(uv > 1) || clip.z < 0 || clip.z > 1) return 1;
    return shadow_map.SampleCmpLevelZero(shadow_sampler, uv, clip.z - 0.00012);
}

float4 mesh_ps(Surface surface) : SV_Target {
    return float4(tonemap(shade_surface(surface)), 1);
}
