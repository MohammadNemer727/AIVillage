-- Drives the founder player through each of its body's abilities on a platform high above the
-- world, and logs BODYTEST PASS or FAIL for each. Run by ../body-test.sh, which also checks what
-- the founder's brain felt. Actions are steered through ai_bridge's test hooks, so each one
-- travels the same path as a decision of the brain.

local founder_name = minetest.settings:get("ai_bridge_founder") or "founder"
local floor_y = 199
local feet = {x = 0, y = floor_y + 0.5, z = 0}
local ahead = {x = 0, y = floor_y + 1, z = 2}
-- Eye pitch that aims at the block two nodes ahead at foot level (about 28 degrees down).
local aim_pitch = -80

local function log(text) minetest.log("action", "BODYTEST " .. text) end

local function hooks() return rawget(_G, "ai_bridge_test") end

local idle = {thrust = 0, turn = 0, strafe = 0, look_pitch = 0, jump = 0, jaw = 0, hand_pulse = 0,
  use_pulse = 0, next_slot = 0}
local action = table.copy(idle)
-- A one-shot pulse (use, next item) is delivered to exactly one decision of the brain.
local pending_pulse

local function act(changes)
  action = table.copy(idle)
  for field, value in pairs(changes or {}) do action[field] = value end
end

local function pulse(field)
  pending_pulse = field
end

-- The action the next decision receives.
local function decided()
  local now = table.copy(action)
  if pending_pulse then
    now[pending_pulse] = 256
    pending_pulse = nil
  end
  return now
end

-- Look-pitch command that moves the eyes toward `target` (8 units per decision at most).
local function aim(target)
  local now = hooks().state.eye_pitch
  return math.max(-256, math.min(256, (target - now) * 32))
end

local function fence_gate()
  for name in pairs(minetest.registered_nodes) do
    if name:find("^mcl_fences:.*fence_gate$") and minetest.registered_nodes[name .. "_open"] then
      return name
    end
  end
end

local function count(player, item)
  local total = 0
  for _, stack in ipairs(player:get_inventory():get_list("main")) do
    if stack:get_name() == item then total = total + stack:get_count() end
  end
  return total
end

local function stand(player)
  player:set_pos(feet)
  player:set_look_horizontal(0)
  player:set_velocity({x = 0, y = 0, z = 0})
end

local function clear_ahead()
  for z = 1, 4 do minetest.set_node({x = 0, y = floor_y + 1, z = z}, {name = "air"}) end
end

local hunger = rawget(_G, "mcl_hunger")
local placed, opened, switched = false, false, false

-- Each phase: setup(player), step(player, seconds) returning true when passed, and a time limit.
local phases = {
  {name = "dig", limit = 6,
    setup = function()
      clear_ahead()
      minetest.set_node(ahead, {name = "mcl_core:dirt"})
    end,
    step = function()
      local pitch = aim(aim_pitch)
      act({look_pitch = pitch, hand_pulse = pitch == 0 and 256 or 0})
      return minetest.get_node(ahead).name == "air"
    end},
  {name = "pickup", limit = 5,
    step = function(player)
      local picked = count(player, "mcl_core:dirt") > 0
      act({thrust = (not picked and player:get_pos().z < 3) and 256 or 0})
      return picked
    end},
  {name = "place", limit = 4,
    setup = function(player)
      stand(player)
      clear_ahead()
      player:get_inventory():set_list("main", {"mcl_core:dirt 2"})
    end,
    step = function(player, seconds)
      act({look_pitch = aim(aim_pitch)})
      if seconds > 0.5 and not placed then placed = true pulse("use_pulse") end
      for z = 1, 4 do
        if minetest.get_node({x = 0, y = floor_y + 1, z = z}).name == "mcl_core:dirt" then
          return count(player, "mcl_core:dirt") == 1
        end
      end
    end},
  {name = "gate", limit = 4,
    setup = function(player)
      stand(player)
      clear_ahead()
      minetest.set_node(ahead, {name = fence_gate()})
    end,
    step = function(_, seconds)
      act({look_pitch = aim(aim_pitch)})
      if seconds > 0.5 and not opened then opened = true pulse("use_pulse") end
      return minetest.get_node(ahead).name == fence_gate() .. "_open"
    end},
  {name = "slot and eat", limit = 6,
    setup = function(player)
      stand(player)
      -- The hand holds dirt; the apples are in the next hotbar slot.
      local hand = player:get_wield_index()
      local list = {}
      list[hand] = "mcl_core:dirt"
      list[hand % player:hud_get_hotbar_itemcount() + 1] = "mcl_core:apple 2"
      player:get_inventory():set_list("main", {})
      for slot, item in pairs(list) do player:get_inventory():set_stack("main", slot, item) end
      hunger.set_hunger(player, 10)
    end,
    step = function(player, seconds)
      act({jaw = seconds > 0.6 and 256 or 0})
      if seconds > 0.3 and not switched then switched = true pulse("next_slot") end
      return count(player, "mcl_core:apple") < 2 and hunger.get_hunger(player) > 10
    end},
  {name = "feel", limit = 3,
    setup = function(player)
      hunger.set_hunger(player, 6)
      player:set_hp(player:get_hp() - 4)
      log("hurt by 4 with hunger 6")
    end,
    step = function(_, seconds)
      act()
      return seconds > 2
    end},
}

local phase, started, ready_at, finished = 0, 0, nil, false

local function begin(player, index)
  phase, started = index, minetest.get_us_time()
  act()
  if phases[index] and phases[index].setup then phases[index].setup(player) end
end

minetest.register_on_mods_loaded(function()
  minetest.after(0, function()
    local corner = {x = 6, y = 6, z = 6}
    minetest.emerge_area(vector.subtract(feet, corner), vector.add(feet, corner), function(_, _, remaining)
      if remaining ~= 0 then return end
      for x = -4, 4 do for z = -4, 4 do
        minetest.set_node({x = x, y = floor_y, z = z}, {name = "mcl_core:stone"})
      end end
      log("platform built")
    end)
  end)
end)

minetest.register_globalstep(function()
  if finished then return end
  minetest.set_timeofday(0.5)
  for _, entity in pairs(minetest.luaentities) do
    if entity.name and entity.name:find("^mobs_mc:") then entity.object:remove() end
  end
  local player = minetest.get_player_by_name(founder_name)
  local test = hooks()
  if not player or not test then return end
  test.override = decided
  -- Let the founder's birth placement finish before taking over.
  ready_at = ready_at or minetest.get_us_time() + 5000000
  if minetest.get_us_time() < ready_at then return end
  if phase == 0 then
    stand(player)
    test.state.eye_pitch = 0
    player:set_hp(20)
    hunger.set_hunger(player, 20)
    player:get_inventory():set_list("main", {})
    begin(player, 1)
    return
  end
  local current = phases[phase]
  local seconds = (minetest.get_us_time() - started) / 1000000
  if current.step(player, seconds) then
    log("PASS " .. current.name)
  elseif seconds > current.limit then
    log("FAIL " .. current.name)
  else
    return
  end
  if phase == #phases then
    finished = true
    act()
    log("DONE")
  else
    begin(player, phase + 1)
  end
end)
