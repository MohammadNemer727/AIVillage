-- Turn diagnostic. The body is held on flat ground and commanded the same turn every decision
-- (the bridge's own reply is overridden), while the server's true yaw is recorded at every
-- observation. If every commanded turn were carried out exactly once, each decision would change
-- yaw by exactly `turn / 32` steps of 2*pi/256.

local settings = minetest.settings
local founder_name = settings:get("ai_bridge_founder") or "founder"
local decisions_total = tonumber(settings:get("turn_diag_decisions")) or 300
local commanded = tonumber(settings:get("turn_diag_turn")) or 256
local arm = settings:get("turn_diag_arm") or "unknown"
local floor_y = 199
local feet = {x = 0, y = floor_y + 0.5, z = 0}
local STEP = 2 * math.pi / 256

local started, finishing, ready_at = false, false, nil
local decision = 0
local yaws, applied = {}, {}

local function finish()
  minetest.safe_file_write(minetest.get_worldpath() .. "/turn-result.json", minetest.write_json({
    schema_version = 1, arm = arm, commanded_turn = commanded,
    expected_steps_per_decision = math.max(-8, math.min(8,
      commanded >= 0 and math.floor(commanded / 32) or -math.floor(-commanded / 32))),
    decisions = decision, yaw_radians = yaws, applied_steps = applied,
  }, false))
  minetest.request_shutdown("turn diagnostic complete", false, 0)
end

minetest.register_on_mods_loaded(function()
  minetest.after(0, function()
    minetest.emerge_area({x = -8, y = floor_y - 4, z = -8}, {x = 8, y = floor_y + 6, z = 8},
      function(_, _, remaining)
        if remaining ~= 0 then return end
        for x = -6, 6 do for z = -6, 6 do
          minetest.set_node({x = x, y = floor_y, z = z}, {name = "mcl_core:stone"})
          for y = 1, 4 do minetest.set_node({x = x, y = floor_y + y, z = z}, {name = "air"}) end
        end end
      end)
  end)
end)

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
  if minetest.get_us_time() < ready_at or started then return end
  started = true
  hooks.paused = false
  player:set_pos(feet)
  player:set_look_horizontal(0)
  player:set_velocity({x = 0, y = 0, z = 0})
  hooks.override = function() return {thrust = 0, turn = commanded, strafe = 0, jump = 0} end
  hooks.before_observation = function(state, body)
    if finishing or not body.player then return end
    decision = decision + 1
    yaws[#yaws + 1] = body.player:get_look_horizontal()
    applied[#applied + 1] = state.applied_steps
    body.player:set_pos(feet) -- hold position so only yaw is under test
    if decision >= decisions_total then finishing = true; minetest.after(0.5, finish) end
  end
end)
