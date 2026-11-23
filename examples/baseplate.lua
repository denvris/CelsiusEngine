local function box(position, size, color, material)
    return {position = position, size = size, color = color, material = material or 0}
end

local platform = box({0, -0.18, 0}, {21, 0.36, 21}, {0.23, 0.32, 0.39}, 1)
local path = box({0, 0.011, -3.1}, {0.08, 0.02, 4.3}, {0.04, 0.58, 0.64}, 2)
local pedestal = box({0, 0.49, -2.7}, {0.49, 0.98, 0.49}, {0.05, 0.11, 0.18})
local beacon = box({0, 1.14, -2.7}, {0.26, 0.26, 0.26}, {0.03, 0.65, 0.70}, 2)
local game = {
    objects = {platform, path, pedestal, beacon},
    step_speed = 2.5,
    movement_limit = 9.5,
    cloud_density = 0.85,
    steps = 0,
    pinches = 0,
    hands = {},
}

for _, x in ipairs({-7, 7}) do
    for _, z in ipairs({-7, 7}) do
        table.insert(game.objects, box({x, 0.7, z}, {0.16, 1.4, 0.16}, {0.07, 0.16, 0.24}))
        table.insert(game.objects, box({x, 1.43, z}, {0.21, 0.13, 0.21}, {0.08, 0.48, 0.56}, 2))
    end
end

local pulse = 0
local pending = nil

function game.on_hand_pose(hand, joints)
    game.hands[hand] = joints
end

function game.process_hand_gesture(gesture, hand)
    local joints = game.hands[hand]
    if gesture == "pinch" and joints and joints.index_tip then
        game.pinches = game.pinches + 1
        pending = {hand = hand, position = joints.index_tip.position}
        pulse = 1
    end
end

function game.on_locomotion_step()
    game.steps = game.steps + 1
end

local sparks = {}
for i = 1, 2 do
    local spark = box({0, -5, 0}, {0.045, 0.045, 0.045}, {1, 0.33, 0.04}, 2)
    sparks[i] = spark
    table.insert(game.objects, spark)
end

local spark_index = 0
function game.on_frame(dt, offset)
    if pending then
        spark_index = spark_index % #sparks + 1
        local spark = sparks[spark_index]
        local p = pending.position
        spark.position = {p[1] + offset[1], p[2] + offset[2], p[3] + offset[3]}
        spark.color = pending.hand == "left" and {0.04, 0.85, 0.92} or {1, 0.35, 0.04}
        beacon.color = spark.color
        pending = nil
    end
    pulse = math.max(0, pulse - dt * 2.8)
    local size = 0.26 + pulse * 0.10
    beacon.size = {size, size, size}
    beacon.yaw = math.sin(game.steps * 0.3 + pulse) * 0.6
end

return game
