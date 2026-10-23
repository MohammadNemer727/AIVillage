-- Real VoxeLibre spatial-memory fixture (8C).
--
-- A walled stone arena. The body is born on a visible relief patch (so it has one relief to
-- remember) and a harm patch lies at a seeded bearing and distance, so every seed is an
-- unfamiliar world. Hunger drains slowly; standing on the relief patch refills it; standing on
-- the harm patch hurts. The bridge is paused until the body is placed, so no motion is
-- integrated before the experiment begins. Only the normal policy acts. Nothing about
-- places, bearings or coordinates is sent to Rust: the body must find the patch and, if it can,
-- remember how to get back to it. The server records true positions only so dead-reckoning
-- error and outcomes can be measured afterwards.

local settings = minetest.settings
local founder_name = settings:get("ai_bridge_founder") or "founder"
local seed = tonumber(settings:get("spatial_seed")) or 0
local total = tonumber(settings:get("spatial_decisions")) or 600
local variant = settings:get("spatial_variant") or "open"
local arm = settings:get("spatial_arm") or "unknown"
local protocol = settings:get("spatial_protocol") or "spatial-8c"
local floor_y = 199
local half = 60
local relief_radius, harm_radius = 4.0, 3.0
local drain_every, start_hunger = 20, 8

local rng = PcgRandom(seed)
-- The body starts on the relief patch, so every individual has one relief to remember.
local relief = {x = 0, z = 0}
local start_angle = rng:next(0, 359) * math.pi / 180
local start_offset = rng:next(0, 20) / 10
local feet = {x = start_offset * math.cos(start_angle), y = floor_y + 0.5,
  z = start_offset * math.sin(start_angle)}
local harm_theta = rng:next(0, 359) * math.pi / 180
local harm_distance = 8 + rng:next(0, 4)
local harm = {x = harm_distance * math.cos(harm_theta), z = harm_distance * math.sin(harm_theta)}
local wall_theta = harm_theta + math.pi * (0.5 + rng:next(0, 10) / 10)
local theta, relief_distance = start_angle, start_offset

local started, finishing, ready_at = false, false, nil
local decision = 0
local hunger_now = start_hunger
local relief_events, relief_entries, harm_decisions, harm_entries = 0, 0, 0, 0
local fed_decisions, starved_decisions, first_relief = 0, 0, nil
local in_relief, in_harm = false, false
local path = {}

local function inside(pos, centre, radius)
  local dx, dz = pos.x - centre.x, pos.z - centre.z
  return dx * dx + dz * dz <= radius * radius
end

local function finish()
  local result = {
    schema_version = 1, protocol = protocol, arm = arm, seed = seed, variant = variant,
    decisions = decision, bearing_degrees = math.floor(theta * 180 / math.pi + 0.5),
    relief_distance = relief_distance, harm_distance = harm_distance,
    relief_events = relief_events, relief_entries = relief_entries,
    first_relief_decision = first_relief,
    fed_decisions = fed_decisions, starved_decisions = starved_decisions,
    harm_decisions = harm_decisions, harm_entries = harm_entries,
    relief_centre_milli = {x = math.floor(relief.x * 1000), z = math.floor(relief.z * 1000)},
    start_milli = {x = math.floor(feet.x * 1000 + 0.5), z = math.floor(feet.z * 1000 + 0.5)},
    path_milli = path,
  }
  minetest.safe_file_write(minetest.get_worldpath() .. "/spatial-result.json",
    minetest.write_json(result, false))
  minetest.request_shutdown("spatial fixture complete", false, 0)
end

local function build()
  local corner = {x = half + 3, y = 6, z = half + 3}
  minetest.emerge_area(vector.subtract(feet, corner), vector.add(feet, corner),
    function(_, _, remaining)
      if remaining ~= 0 then return end
      for x = -half, half do
        for z = -half, half do
          local edge = math.abs(x) == half or math.abs(z) == half
          minetest.set_node({x = x, y = floor_y, z = z}, {name = "mcl_core:stone"})
          for y = 1, 3 do
            minetest.set_node({x = x, y = floor_y + y, z = z},
              {name = edge and "mcl_core:stone" or "air"})
          end
        end
      end
      for x = -half + 1, half - 1 do
        for z = -half + 1, half - 1 do
          local pos = {x = x, y = floor_y, z = z}
          if inside(pos, relief, relief_radius) then
            minetest.set_node(pos, {name = "mcl_core:goldblock"})
          elseif inside(pos, harm, harm_radius) then
            minetest.set_node(pos, {name = "mcl_core:redsand"})
          end
        end
      end
      if variant == "wall" then
        -- A wall across the way back to the relief patch, nine blocks out at a seeded bearing.
        local ux, uz = math.cos(wall_theta), math.sin(wall_theta)
        local cx, cz = ux * 9, uz * 9
        for step = -5, 5 do
          local pos = {x = math.floor(cx - uz * step + 0.5), y = floor_y + 1,
            z = math.floor(cz + ux * step + 0.5)}
          for y = 0, 2 do
            minetest.set_node({x = pos.x, y = pos.y + y, z = pos.z}, {name = "mcl_core:stone"})
          end
        end
      end
    end)
end

minetest.register_on_mods_loaded(function() minetest.after(0, build) end)

minetest.register_globalstep(function()
  if finishing then return end
  minetest.set_timeofday(0.5)
  for _, entity in pairs(minetest.luaentities) do
    if entity.name and entity.name:find("^mobs_mc:") then entity.object:remove() end
  end
  local player = minetest.get_player_by_name(founder_name)
  local hooks = rawget(_G, "ai_bridge_test")
  if hooks then hooks.paused = not started end
  if not player or not hooks or not hooks.state then return end
  ready_at = ready_at or minetest.get_us_time() + 8000000
  if minetest.get_us_time() < ready_at then return end
  if started then return end
  started = true
  hooks.paused = false
  player:set_pos(feet)
  player:set_look_horizontal(0)
  player:set_velocity({x = 0, y = 0, z = 0})
  player:set_hp(20)
  local hunger = rawget(_G, "mcl_hunger")
  if hunger then hunger.set_hunger(player, hunger_now) end
  hooks.before_observation = function(_, body)
    if finishing or not body.player then return end
    local pos = body.player:get_pos()
    decision = decision + 1
    path[#path + 1] = {math.floor(pos.x * 1000 + 0.5), math.floor(pos.z * 1000 + 0.5)}
    -- Floor at 1: starvation damage is not something a place does, so it must not be felt as one.
    if decision % drain_every == 0 then hunger_now = math.max(1, hunger_now - 1) end
    local now_relief = inside(pos, relief, relief_radius)
    if now_relief then
      if not in_relief then relief_entries = relief_entries + 1 end
      if hunger_now < 20 then
        hunger_now = math.min(20, hunger_now + 4)
        relief_events = relief_events + 1
        first_relief = first_relief or decision
      end
    end
    in_relief = now_relief
    local now_harm = inside(pos, harm, harm_radius)
    if now_harm then
      if not in_harm then harm_entries = harm_entries + 1 end
      harm_decisions = harm_decisions + 1
      body.player:set_hp(math.max(1, body.player:get_hp() - 2))
    end
    in_harm = now_harm
    if hunger then hunger.set_hunger(body.player, hunger_now) end
    if hunger_now >= 10 then fed_decisions = fed_decisions + 1 end
    if hunger_now <= 1 then starved_decisions = starved_decisions + 1 end
    if decision >= total then
      finishing = true
      minetest.after(0.5, finish)
    end
  end
end)
