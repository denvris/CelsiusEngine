local lab = {
    objects = {},
    ray_tracing = true,
    cloud_density = 0.72,
    step_speed = 2.2,
    movement_limit = 8.0,
}

local function box(position, size, color, material)
    table.insert(lab.objects, {position = position, size = size, color = color, material = material})
end

box({0, -0.14, -2.5}, {19, 0.28, 17}, {0.37, 0.43, 0.49}, 15)
box({0, 1.76, -6.66}, {8.0, 2.00, 0.13}, {0.035, 0.055, 0.086}, 0)
box({0, 2.09, -6.58}, {7.5, 0.59, 0.025}, {1.0, 1.0, 1.0}, 20)
box({0, 0.93, -6.57}, {7.5, 0.06, 0.035}, {0.07, 0.77, 0.95}, 2)
box({-4.14, 1.36, -6.53}, {0.12, 2.5, 0.15}, {0.07, 0.77, 0.95}, 2)
box({4.14, 1.36, -6.53}, {0.12, 2.5, 0.15}, {1.0, 0.35, 0.08}, 2)
box({-4.4, 1.46, -0.5}, {0.18, 2.9, 0.28}, {0.02, 0.39, 0.98}, 2)
box({4.4, 1.46, -0.5}, {0.18, 2.9, 0.28}, {1.0, 0.15, 0.045}, 2)

-- The high-contrast panel behind the viewer appears in the mirror with scene rays enabled.
box({0, 1.67, 3.25}, {7.5, 1.25, 0.11}, {1.0, 0.79, 0.42}, 20)
box({-1.67, 2.7, 2.75}, {1.60, 0.23, 0.14}, {0.52, 0.83, 1.0}, 2)
box({1.67, 2.7, 2.75}, {1.60, 0.23, 0.14}, {1.0, 0.81, 0.48}, 2)

local finishes = {
    {10, {0.89, 0.94, 1.0}},   -- Chrome
    {11, {0.86, 0.54, 0.24}},  -- Brushed metal
    {12, {0.77, 0.94, 1.0}},   -- Glass
    {13, {0.055, 0.32, 0.57}}, -- Glazed ceramic
    {14, {1.0, 1.0, 1.0}},     -- Varnished wood
    {15, {0.72, 0.76, 0.82}},  -- Concrete
}

for i, finish in ipairs(finishes) do
    local x = (i - 3.5) * 1.24
    box({x, 0.37, -4.45}, {0.94, 0.74, 0.91}, {0.031, 0.049, 0.073}, 0)
    box({x, 0.64, -3.989}, {0.6, 0.024, 0.012}, {0.09, 0.56, 0.72}, 2)
    table.insert(lab.objects, {
        shape = "sphere",
        position = {x, 1.20, -4.45},
        radius = 0.46,
        color = finish[2],
        material = finish[1],
    })
end

return lab
