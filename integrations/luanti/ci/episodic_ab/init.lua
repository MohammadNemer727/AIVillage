-- Real VoxeLibre paired fixture for the preregistered episodic policy-only A/B.
--
-- The founder acts through its normal player body and bridge. At each decision, backward thrust
-- returns one unit of the body's ordinary hunger resource; every other thrust choice causes one
-- heart of ordinary player damage. No action is overridden and no outcome label enters Rust.

local founder_name = minetest.settings:get("ai_bridge_founder") or "founder"
local trials = tonumber(minetest.settings:get("episodic_ab_trials")) or 16
local seed = tonumber(minetest.settings:get("episodic_ab_seed")) or 0
local arm = minetest.settings:get("episodic_ab_arm") or "unknown"
local floor_y = 199
local feet = {x = 0, y = floor_y + 0.5, z = 0}
local started, finishing = false, false
local ready_at
local last_sequence
local injuries, resource_returns = 0, 0
local choices = {}

local function log(event)
  minetest.log("action", "EPISODIC_AB " .. minetest.write_json(event))
end

local function finish()
  local result = {
    schema_version = 1,
    protocol = "episodic-policy-live-ab-v1",
    arm = arm,
    seed = seed,
    trials = #choices,
    injuries = injuries,
    resource_returns = resource_returns,
    choices = choices,
  }
  minetest.safe_file_write(
    minetest.get_worldpath() .. "/episodic-ab-result.json",
    minetest.write_json(result, true)
  )
  log({event = "done", arm = arm, seed = seed, trials = #choices,
    injuries = injuries, resource_returns = resource_returns})
  minetest.request_shutdown("episodic A/B complete", false, 0)
end

minetest.register_on_mods_loaded(function()
  minetest.after(0, function()
    local corner = {x = 5, y = 5, z = 5}
    minetest.emerge_area(vector.subtract(feet, corner), vector.add(feet, corner),
      function(_, _, remaining)
        if remaining ~= 0 then return end
        for x = -4, 4 do
          for z = -4, 4 do
            minetest.set_node({x = x, y = floor_y, z = z}, {name = "mcl_core:stone"})
          end
        end
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
  if not player or not hooks or not hooks.state then return end
  ready_at = ready_at or minetest.get_us_time() + 5000000
  if minetest.get_us_time() < ready_at then return end
  if not started then
    started = true
    player:set_pos(feet)
    player:set_look_horizontal(0)
    player:set_velocity({x = 0, y = 0, z = 0})
    player:set_hp(20)
    local hunger = rawget(_G, "mcl_hunger")
    if hunger then hunger.set_hunger(player, 2) end
    last_sequence = hooks.state.sequence
    log({event = "start", arm = arm, seed = seed, sequence = last_sequence})
    return
  end
  if hooks.state.sequence == last_sequence then return end
  last_sequence = hooks.state.sequence

  -- `held` is the just-returned Rust action. Apply its world consequence during its normal hold
  -- interval, before the subsequent observation scores that consequence.
  local thrust = hooks.state.held.thrust or 0
  if thrust < 0 then
    local hunger = rawget(_G, "mcl_hunger")
    local before = hunger and hunger.get_hunger(player) or 0
    if hunger then hunger.set_hunger(player, math.min(20, before + 1)) end
    resource_returns = resource_returns + 1
    table.insert(choices, {sequence = last_sequence, thrust = thrust, outcome = "resource_return"})
  else
    player:set_hp(player:get_hp() - 1)
    injuries = injuries + 1
    table.insert(choices, {sequence = last_sequence, thrust = thrust, outcome = "injury"})
  end
  if #choices >= trials then
    finishing = true
    minetest.after(0.2, finish)
  end
end)
