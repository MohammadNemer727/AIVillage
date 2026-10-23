-- Preregistered real VoxeLibre repeated-choice fixture for the episodic live-choice protocols
-- (episodic-policy-live-choice-v5 and Experiment 003).
--
-- Only the normal affordance policy acts. Forward and backward thrust are the two eligible
-- choices. A test-only hook applies their bodily consequence after the full action hold and
-- immediately before the following observation. No action or outcome label is sent to Rust.

local founder_name = minetest.settings:get("ai_bridge_founder") or "founder"
local warmup = tonumber(minetest.settings:get("episodic_choice_warmup")) or 24
local scored = tonumber(minetest.settings:get("episodic_choice_scored")) or 40
local seed = tonumber(minetest.settings:get("episodic_ab_seed")) or 0
local arm = minetest.settings:get("episodic_ab_arm") or "unknown"
local protocol = minetest.settings:get("episodic_choice_protocol") or "episodic-policy-live-choice-v5"
local floor_y = 199
local feet = {x = 0, y = floor_y + 0.5, z = 0}
local started, finishing = false, false
local ready_at
local eligible, resource_losses, resource_returns = 0, 0, 0
local warmup_forward, warmup_backward = 0, 0
local observed_fullness_increases, observed_fullness_decreases = 0, 0
local action_distribution = {}
local choices = {}

local function set_common_pose(player)
  player:set_pos(feet)
  player:set_velocity({x = 0, y = 0, z = 0})
end

local function set_initial_body(player)
  set_common_pose(player)
  player:set_hp(20)
  local hunger = rawget(_G, "mcl_hunger")
  if hunger then hunger.set_hunger(player, 10) end
end

local function finish()
  local result = {
    schema_version = 1,
    protocol = protocol,
    arm = arm,
    seed = seed,
    warmup_eligible_choices = warmup,
    scored_eligible_choices = #choices,
    warmup_forward = warmup_forward,
    warmup_backward = warmup_backward,
    baseline_exposure_valid = warmup_forward >= 4 and warmup_backward >= 4,
    resource_losses = resource_losses,
    resource_returns = resource_returns,
    observed_fullness_increases = observed_fullness_increases,
    observed_fullness_decreases = observed_fullness_decreases,
    action_distribution = action_distribution,
    choices = choices,
  }
  minetest.safe_file_write(
    minetest.get_worldpath() .. "/episodic-choice-result.json",
    minetest.write_json(result, true)
  )
  minetest.request_shutdown("episodic choice A/B complete", false, 0)
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
    set_initial_body(player)
    hooks.before_observation = function(state, body)
      if finishing or not started or not body.player then return end
      set_common_pose(body.player)
      local thrust = state.held.thrust or 0
      local key = tostring(thrust)
      action_distribution[key] = (action_distribution[key] or 0) + 1
      if thrust ~= 256 and thrust ~= -256 then return end

      eligible = eligible + 1
      local phase = eligible <= warmup and "warmup" or "scored"
      local hunger = rawget(_G, "mcl_hunger")
      local before = hunger and hunger.get_hunger(body.player) or 0
      local outcome, after
      if thrust == 256 then
        outcome = "resource_loss"
        if phase == "warmup" then warmup_forward = warmup_forward + 1 end
        after = math.max(0, before - 4)
      else
        outcome = "resource_return"
        if phase == "warmup" then warmup_backward = warmup_backward + 1 end
        after = math.min(20, before + 4)
      end
      if hunger then hunger.set_hunger(body.player, after) end
      if after > before then
        observed_fullness_increases = observed_fullness_increases + 1
      elseif after < before then
        observed_fullness_decreases = observed_fullness_decreases + 1
      end
      if phase == "scored" then
        if outcome == "resource_loss" then resource_losses = resource_losses + 1
        else resource_returns = resource_returns + 1 end
        table.insert(choices, {
          action_sequence = state.sequence - 1,
          observed_sequence = state.sequence,
          thrust = thrust,
          outcome = outcome,
          fullness_before = before,
          fullness_after = after,
        })
      end
      if #choices >= scored then
        finishing = true
        minetest.after(0.5, finish)
      end
    end
  end
end)
