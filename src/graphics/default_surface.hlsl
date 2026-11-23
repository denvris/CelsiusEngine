float3 shade_surface(Surface s) {
    float3 normal = normalize(s.normal);
    float3 base = s.color;
    if (s.material == 1 && normal.y > 0.5) {
        float2 grid = s.world.xz;
        float2 grid_distance = abs(frac(grid + 0.5) - 0.5) / max(fwidth(grid), 0.001);
        float coverage = 1 - saturate(min(grid_distance.x, grid_distance.y));
        base = lerp(base, base * 0.40, coverage);
    }
    float3 view = normalize(camera.xyz - s.world);
    float3 half_vec = normalize(view + sun);
    float diffuse = saturate(dot(normal, sun));
    float specular = pow(saturate(dot(normal, half_vec)), 38) * 0.18;
    float3 ambient = settings.x > 0.5 ? min(environment(normal), 1.5) : float3(0.32, 0.40, 0.52);
    float visibility = sunlight_visibility(s.world, normal);
    float3 lit = base * ambient + visibility * diffuse * (base * float3(0.81, 0.74, 0.64) + specular);
    if (s.material == 2) lit += base * 1.25;
    float fog = 1 - exp(-length(camera.xyz - s.world) * 0.013);
    return lerp(lit, environment(normalize(s.world - camera.xyz)), fog);
}
