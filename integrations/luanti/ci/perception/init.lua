-- Controlled perception test (Experiment 005E, part P). A walled flat arena in which one entity
-- approaches, retreats, or approaches behind a stone wall, or a real zombie approaches at night,
-- while the body is held in place looking straight ahead. The fixture records only the
-- ground truth the retina is checked against (distance, bearing, closing speed); the retina
-- frames themselves are the ordinary ones the bridge trace already holds. Nothing here reaches Rust.

local settings = minetest.settings
local founder_name = settings:get("ai_bridge_founder") or "founder"
local seed = tonumber(settings:get("perception_seed")) or 0
local mode = settings:get("perception_mode") or "approach"
local total = tonumber(settings:get("perception_decisions")) or 100
local floor_y = 199
local half = 20
local feet = {x = 0, y = floor_y + 0.5, z = 0}
local speed = 1.0 -- blocks per second, 0.1 per decision
local far, near = 7.0, 1.2

local rng = PcgRandom(seed)
local bearing = (rng:next(0, 50) - 25) * math.pi / 180 -- left of straight ahead, +/-25 degrees

local started, finishing, ready_at, built = false, false, nil, false
local decision = 0
local truth = {}
local object

-- A body-sized puppet with a zombie's skin, so that what the eye reports is comparable with a real
-- zombie. It carries no behaviour and is harmless; the fixture moves it on a fixed schedule.
minetest.register_entity("ai_bridge_perception:target", {
  initial_properties = {physical = true, collide_with_objects = false, collisionbox = {-0.3, 0, -0.3, 0.3, 1.95, 0.3},
    visual = "cube", visual_size = {x = 0.6, y = 1.95}, static_save = false,
    textures = {"mobs_mc_zombie.png", "mobs_mc_zombie.png", "mobs_mc_zombie.png",
      "mobs_mc_zombie.png", "mobs_mc_zombie.png", "mobs_mc_zombie.png"}},
})

local function finish()
  minetest.safe_file_write(minetest.get_worldpath() .. "/perception-result.json", minetest.write_json({
    schema_version = 1, mode = mode, seed = seed, decisions = decision,
    bearing_degrees = bearing * 180 / math.pi, truth = truth,
  }, false))
  minetest.request_shutdown("perception complete", false, 0)
end

local function direction() return {x = -math.sin(bearing), y = 0, z = math.cos(bearing)} end

local function build()
  local corner = {x = half + 3, y = 6, z = half + 3}
  minetest.emerge_area(vector.subtract(feet, corner), vector.add(feet, corner),
    function(_, _, remaining)
      if remaining ~= 0 then return end
      for x = -half, half do
        for z = -half, half do
          local edge = math.abs(x) == half or math.abs(z) == half
          minetest.set_node({x = x, y = floor_y, z = z}, {name = "mcl_core:stone"})
          for y = 1, 5 do
            minetest.set_node({x = x, y = floor_y + y, z = z}, {name = edge and "mcl_core:stone" or "air"})
          end
        end
      end
      if mode == "occluded" then
        -- A wall two blocks away across the line of sight, wide and tall enough to hide the object.
        local d, side = direction(), {x = math.cos(bearing), y = 0, z = math.sin(bearing)}
        for u = -5, 5, 0.5 do
          for h = 1, 4 do
            minetest.set_node(vector.round({x = d.x * 2 + side.x * u, y = floor_y + h, z = d.z * 2 + side.z * u}),
              {name = "mcl_core:stone"})
          end
        end
      end
      built = true
    end)
end

minetest.register_on_mods_loaded(function() minetest.after(0, build) end)

local function object_distance(t)
  -- Distance of the moving entity from the body at the start of a decision `t` steps in.
  if mode == "retreat" then return math.min(far, near + speed * 0.1 * t) end
  if mode == "blank" then return far end
  return math.max(near + (mode == "occluded" and 1.8 or 0), far - speed * 0.1 * math.max(t - 10, 0))
end

minetest.register_globalstep(function()
  if finishing then return end
  minetest.set_timeofday(mode == "zombie" and 0.0 or 0.5)
  for _, entity in pairs(minetest.luaentities) do
    if entity.name and entity.name:find("^mobs_mc:") and mode ~= "zombie" then entity.object:remove() end
  end
  local player = minetest.get_player_by_name(founder_name)
  local hooks = rawget(_G, "ai_bridge_test")
  if hooks then hooks.paused = not started end
  if not player or not hooks or not hooks.state then return end
  ready_at = ready_at or minetest.get_us_time() + 8000000
  if minetest.get_us_time() < ready_at or started or not built then return end
  started = true
  hooks.paused = false
  player:set_pos(feet)
  player:set_look_horizontal(0)
  player:set_look_vertical(0)
  player:set_velocity({x = 0, y = 0, z = 0})
  local hunger = rawget(_G, "mcl_hunger")
  if hunger then hunger.set_hunger(player, 20) end
  local d = direction()
  if mode == "zombie" then
    object = minetest.add_entity({x = d.x * far, y = floor_y + 1, z = d.z * far}, "mobs_mc:zombie")
  elseif mode ~= "blank" then
    local start = object_distance(0)
    object = minetest.add_entity({x = d.x * start, y = floor_y + 0.6, z = d.z * start}, "ai_bridge_perception:target")
  end
  hooks.before_observation = function(_, body)
    if finishing or not body.player then return end
    decision = decision + 1
    -- The body is held in place and facing straight ahead: this tests what the eye reports about
    -- an entity, not how the body moves. Its own motion is therefore zero in every frame.
    body.player:set_hp(20)
    body.player:set_pos(feet)
    body.player:set_velocity({x = 0, y = 0, z = 0})
    body.player:set_look_horizontal(0)
    body.player:set_look_vertical(0)
    if hunger then hunger.set_hunger(body.player, 20) end
    local pos, yaw = body.player:get_pos(), body.player:get_look_horizontal()
    local row = {yaw_degrees = yaw * 180 / math.pi}
    if object and object:get_pos() then
      if mode ~= "zombie" then
        -- Kinematic puppet: the position follows the schedule and the velocity is set to match,
        -- so what the eye sees moving is what is really moving.
        local before, after = object_distance(decision - 1), object_distance(decision)
        object:set_pos({x = d.x * before, y = floor_y + 0.6, z = d.z * before})
        object:set_velocity(vector.multiply(d, (after - before) / 0.1))
      end
      local at = object:get_pos()
      local dx, dz = at.x - pos.x, at.z - pos.z
      local forward = {x = -math.sin(yaw), z = math.cos(yaw)}
      local left = {x = -math.cos(yaw), z = -math.sin(yaw)}
      local velocity = object:get_velocity() or {x = 0, y = 0, z = 0}
      local length = math.sqrt(dx * dx + dz * dz)
      row.distance = length
      row.bearing_left_degrees = math.deg(math.atan2(dx * left.x + dz * left.z, dx * forward.x + dz * forward.z))
      row.closing = -((velocity.x * dx + velocity.z * dz) / math.max(length, 0.01))
    end
    truth[#truth + 1] = row
    if decision >= total then finishing = true; minetest.after(0.5, finish) end
  end
end)
