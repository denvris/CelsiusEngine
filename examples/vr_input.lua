local gameplay = { abilities_cast = 0, steps = 0 }
local hands = {}

function gameplay.on_hand_pose(hand, joints)
    hands[hand] = joints
end

function gameplay.process_hand_gesture(gesture, hand)
    local joints = hands[hand]
    local index = joints and joints.index_tip
    if gesture == "pinch" and index and index.position[2] > 1 then
        gameplay.abilities_cast = gameplay.abilities_cast + 1
    end
end

function gameplay.on_locomotion_step()
    gameplay.steps = gameplay.steps + 1
end

return gameplay
