-- Matched live arena (Experiment 005). A walled flat stone arena, either in daylight with no mobs
-- or at night with one zombie placed at a seeded bearing and distance. The body is placed at the
-- centre, held at a fixed hunger so that hunger does not move it, and the bridge is paused until
-- it is placed. Position and health are recorded at every decision, nothing else is sent to Rust.

local settings = minetest.settings
local founder_name = settings:get("ai_bridge_founder") or "founder"
local seed = tonumber(settings:get("arena_seed")) or 0
local total = tonumber(settings:get("arena_decisions")) or 400
local zombie_distance = tonumber(settings:get("arena_zombie_distance")) or 0
local night = (settings:get("arena_night") or "0") == "1"
local arm = settings:get("arena_arm") or "unknown"
local floor_y = 199
local half = 60
local feet = {x = 0, y = floor_y + 0.5, z = 0}

local rng = PcgRandom(seed)
local bearing = rng:next(0, 359) * math.pi / 180

local started, finishing, ready_at = false, false, nil
local decision = 0
local path, health = {}, {}
local died_at, zombie_hits = nil, 0
local zombie

local function finish()
  minetest.safe_file_write(minetest.get_worldpath() .. "/arena-result.json", minetest.write_json({
    schema_version = 1, arm = arm, seed = seed, decisions = decision,
    zombie_distance = zombie_distance, night = night, bearing_degrees = math.floor(bearing * 180 / math.pi + 0.5),
    died_at_decision = died_at, path_milli = path, health = health,
  }, false))
  minetest.request_shutdown("arena complete", false, 0)
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
          for y = 1, 4 do
            minetest.set_node({x = x, y = floor_y + y, z = z}, {name = edge and "mcl_core:stone" or "air"})
          end
        end
      end
    end)
end

minetest.register_on_mods_loaded(function() minetest.after(0, build) end)

minetest.register_on_dieplayer(function(player)
  -- A dead body makes no more decisions, so nothing else would ever end the run.
  if player:get_player_name() == founder_name and not died_at then
    died_at = decision
    finishing = true
    minetest.after(0.5, finish)
  end
end)

minetest.register_globalstep(function()
  if finishing then return end
  minetest.set_timeofday(night and 0.0 or 0.5)
  if zombie_distance == 0 then
    for _, entity in pairs(minetest.luaentities) do
      if entity.name and entity.name:find("^mobs_mc:") then entity.object:remove() end
    end
  end
  local player = minetest.get_player_by_name(founder_name)
  local hooks = rawget(_G, "ai_bridge_test")
  if hooks then hooks.paused = not started end
  if not player or not hooks or not hooks.state then return end
  ready_at = ready_at or minetest.get_us_time() + 8000000
  if minetest.get_us_time() < ready_at or started then return end
  started = true
  hooks.paused = false
  player:set_pos(feet)
  player:set_look_horizontal(0)
  player:set_velocity({x = 0, y = 0, z = 0})
  player:set_hp(20)
  local hunger = rawget(_G, "mcl_hunger")
  if hunger then hunger.set_hunger(player, 20) end
  if zombie_distance > 0 then
    zombie = minetest.add_entity({
      x = zombie_distance * math.cos(bearing), y = floor_y + 1, z = zombie_distance * math.sin(bearing),
    }, "mobs_mc:zombie")
  end
  hooks.before_observation = function(_, body)
    if finishing or not body.player then return end
    local pos = body.player:get_pos()
    decision = decision + 1
    path[#path + 1] = {math.floor(pos.x * 1000 + 0.5), math.floor(pos.z * 1000 + 0.5)}
    health[#health + 1] = body.player:get_hp()
    if hunger then hunger.set_hunger(body.player, 20) end
    if decision >= total or died_at then finishing = true; minetest.after(0.5, finish) end
  end
end)
