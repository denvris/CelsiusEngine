local progression = {}

function progression.award_xp(current_xp, earned_xp)
    local total_xp = current_xp + earned_xp
    local level = 1 + math.floor(total_xp / 100)
    return total_xp, level
end

return progression
