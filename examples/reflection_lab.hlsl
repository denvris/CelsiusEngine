float3 lab_lighting(Surface s, float3 n, float3 view, float3 color, float roughness) {
    float visibility = sunlight_visibility(s.world, n);
    float3 ambient = min(environment(n), 1.3) * 0.22 + float3(0.08, 0.09, 0.10);
    float3 half_dir = normalize(view + sun);
    float spec = pow(saturate(dot(n, half_dir)), lerp(160, 12, roughness));
    return color * (ambient + visibility * saturate(dot(n, sun)) * 0.76) + visibility * spec * 0.24;
}

float3 shade_surface(Surface s) {
    float3 n = normalize(s.normal);
    float3 view = normalize(camera.xyz - s.world);
    float3 origin = s.world + n * 0.006;
    float3 reflected = reflect(-view, n);
    float facing = saturate(dot(n, view));
    float grazing = pow(1 - facing, 5);
    float roughness = 0.52;
    float3 color = s.color;
    float material = s.material;

    if (material == 20) {
        return diagnostic_color(s.world, color, material, fwidth(s.world.x)) * 1.18;
    }
    if (material == 2) return color * 1.45;
    if (material == 10) {
        return trace_radiance(origin, reflected) * color;
    }
    if (material == 11) {
        // Fixed two-tap anisotropic lobe, stable between frames.
        float3 tangent = cross(float3(0, 1, 0), n);
        float3 a = normalize(reflected + tangent * 0.14);
        float3 b = normalize(reflected - tangent * 0.14);
        return color * (trace_radiance(origin, a) + trace_radiance(origin, b)) * 0.43
            + color * 0.11;
    }
    if (material == 12) {
        const float ior = 1.50;
        float3 inner = refract(-view, n, 1.0 / ior);
        float3 inside = s.world - n * 0.006;
        float thickness = max(0, sphere_distance(inside, inner, s.sphere));
        float3 exit_position = inside + thickness * inner;
        float3 exit_normal = normalize(exit_position - s.sphere.xyz);
        float3 outgoing = refract(inner, -exit_normal, ior);
        float3 reflection = trace_radiance(origin, reflected);
        if (dot(outgoing, outgoing) < 0.01) return reflection;
        float3 transmission = trace_radiance(exit_position + exit_normal * 0.006, outgoing);
        float3 absorption = exp(-float3(0.24, 0.07, 0.05) * thickness);
        return lerp(transmission * absorption, reflection, 0.04 + 0.96 * grazing);
    }
    if (material == 13) {
        float bands = 0.55 + 0.45 * sin(s.world.y * 15);
        return lerp(lab_lighting(s, n, view, color * bands, 0.25),
                    trace_radiance(origin, reflected), 0.07 + grazing * 0.55);
    }
    if (material == 14) {
        float2 uv = float2(atan2(n.x, -n.z) * 0.15915494 + 0.5, acos(clamp(n.y, -1, 1)) * 0.31830989);
        float3 grain = lerp(float3(0.37, 0.13, 0.042), float3(0.18, 0.044, 0.018),
                            noise(float3(uv.x * 17, uv.y * 190, 2)));
        color = material_maps.z > 0.5 ? wood_color.Sample(material_sampler, uv).rgb : grain;
        roughness = material_maps.w > 0.5 ? wood_roughness.Sample(material_sampler, uv).r : 0.29;
        return lerp(lab_lighting(s, n, view, color, roughness),
                    trace_radiance(origin, reflected) * 0.45, (0.12 + 0.45 * grazing) * (1 - roughness));
    }
    if (material == 15) {
        float2 uv = s.world.xz * 0.48;
        if (s.sphere.w > 0) {
            uv = float2(atan2(n.x, -n.z) * 0.15915494 + 0.5, acos(clamp(n.y, -1, 1)) * 0.31830989);
        }
        float grit = 0.74 + 0.26 * noise(float3(uv * 130, 1));
        color *= material_maps.x > 0.5 ? concrete_color.Sample(material_sampler, uv).rgb * 1.8 : grit;
        roughness = material_maps.y > 0.5 ? concrete_roughness.Sample(material_sampler, uv).r : 0.84;
    }
    return lab_lighting(s, n, view, color, roughness);
}
