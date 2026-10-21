-- One physical founder and an intentionally narrow, loopback-only bridge protocol.
-- No coordinates, node names, recipes, rewards, or world facts cross into Rust.
local palette = dofile(minetest.get_modpath("ai_bridge") .. "/palette_generated.lua").colours
local protocol, agent_id = 2, 1
local step_seconds, request_timeout = 0.1, 0.5
local endpoint = minetest.settings:get("ai_bridge_endpoint") or "http://127.0.0.1:8481/v1/transition"
local http = minetest.request_http_api()
local mode = minetest.settings:get("ai_bridge_mode") or "observe"
if mode ~= "observe" and mode ~= "smoke" and mode ~= "demo-motion" then
  minetest.log("error", "ai_bridge: invalid ai_bridge_mode; using observe")
  mode = "observe"
end
local scripted = mode == "smoke" or mode == "demo-motion"
local observer_name = minetest.settings:get("ai_bridge_observer") or "observer"

local function clamp(value, low, high)
  return math.max(low, math.min(high, value))
end
local function byte(value) return math.floor(clamp(value, 0, 255) + 0.5) end
local function signed(value)
  value = clamp(value, -255, 255)
  return value < 0 and math.ceil(value - 0.5) or math.floor(value + 0.5)
end
local function empty_senses()
  local retinal = {}
  for ray = 1, 16 do retinal[ray] = { red = 120, green = 170, blue = 255, brightness = 204, motion = 0, occlusion = 0 } end
  return { retinal = retinal, chemicals = {0, 0, 0, 0}, vibration = 0, touch = 0,
    joint_position = 0, muscle_tension = 0, acceleration = 0, orientation = 0,
    energy = 0, gut_pressure = 0, toxin_stress = 0, hydration = 0, fatigue = 0, damage = 0 }
end

-- All visible palette nodes are solid and pointable. Do not add pointable non-solid decorations:
-- Luanti raycasts would stop at them before transparent-ray semantics are explicitly designed.
local function register_palette_node(name, rgb)
  minetest.register_node(name, {
    description = name, tiles = {"ai_bridge_base.png^[colorize:#" .. string.format("%02x%02x%02x", rgb[1], rgb[2], rgb[3]) .. ":255"},
    walkable = true, pointable = true, paramtype = "light", groups = {cracky = 1},
  })
end
for name, rgb in pairs(palette) do register_palette_node(name, rgb) end
-- Luanti's bundled `blank.png` is fully transparent, so colorizing it makes an invisible
-- material.  Use this mod's opaque original base texture instead. Every room node must also
-- store light in param1; `fix_light` below recalculates it after installation.
-- Observer-only architecture stays outside the founder's horizontal eye-ray band, so it cannot
-- change the V2 retinal fixture.
minetest.register_node("ai_bridge:observer_glass", {
  description = "Observer ceiling", drawtype = "glasslike",
  tiles = {"ai_bridge_base.png^[colorize:#a8dfff:80"}, walkable = true, pointable = false,
  paramtype = "light", sunlight_propagates = true, groups = {cracky = 1},
})
minetest.register_node("ai_bridge:observer_light", {
  description = "Observer room light", drawtype = "glasslike",
  tiles = {"ai_bridge_base.png^[colorize:#fff1b0:255"}, walkable = false, pointable = false,
  paramtype = "light", sunlight_propagates = true, light_source = 14,
  groups = {not_in_creative_inventory = 1},
})

local state = { sequence = 0, server_step = 0, pending = false, pending_at = 0, request_token = 0,
  held = { thrust = 0, turn = 0, jaw = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} },
  applied_step = 0, applied_steps = 0, elapsed = 0, previous_velocity = {x = 0, z = 0}, touch = 0, status = "healthy",
  smoke = mode == "smoke", diagnostics = nil, latency_ms = 0, founder = nil, observer_resets = 0 }

local function smoke_log(event)
  if state.smoke then minetest.log("action", "AI_BRIDGE_SMOKE " .. minetest.write_json(event)) end
end

local function zero_action(reason)
  state.held = { thrust = 0, turn = 0, jaw = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} }
  state.status = reason
  smoke_log({event = "safe_zero", status = reason, thrust = state.held.thrust, turn = state.held.turn})
end

local function pose(self)
  local position = self.object:get_pos()
  return {x_millinode = math.floor(position.x * 1000 + 0.5), y_millinode = math.floor(position.y * 1000 + 0.5),
    z_millinode = math.floor(position.z * 1000 + 0.5), yaw_milliradian = math.floor(self.object:get_yaw() * 1000 + 0.5)}
end

local function hud_text()
  local diagnostics = state.diagnostics or {}
  local physiology = diagnostics.frozen_physiology or {}
  local actuator = scripted and "fixture action applied" or "Rust action applied"
  return table.concat({
    "AI Village observer — explore with normal movement and jump; mouse looks",
    "FOUNDER below: amber-faced green body; observe mode motors accept only Rust bridge responses",
    "decision tick: " .. tostring(diagnostics.decision_tick or "waiting") .. "   Luanti step: " .. state.server_step,
    actuator .. ": thrust " .. tostring(state.held.thrust or 0) .. " turn " .. tostring(state.held.turn or 0),
    "latency: " .. tostring(state.latency_ms) .. " ms   transport: " .. state.status,
    "receptors active/used: green mean, brightness mean, interoception",
    "active/unused: red, blue, ray direction; dead: chemicals, vibration",
    "brain: " .. (diagnostics.learned and "learning" or "blank") .. " fp " .. tostring(diagnostics.brain_fingerprint or "—")
      .. " mod " .. tostring(diagnostics.mean_modulator_activity or "—"),
    "frozen physiology: E " .. tostring(physiology.energy or 120) .. " G " .. tostring(physiology.gut or 120)
      .. " H " .. tostring(physiology.hydration or 120) .. " F " .. tostring(physiology.fatigue or 0)
      .. " T " .. tostring(physiology.toxin or 0) .. " D " .. tostring(physiology.damage or 0),
    "observer-only resets: " .. tostring(state.observer_resets) .. "  (/founder_reset; does not steer or retrain)",
  }, "\n")
end

local function update_observer_hud()
  local player = minetest.get_player_by_name(observer_name)
  if player then
    local id = player:get_meta():get_int("ai_bridge_hud")
    if id == 0 then
      id = player:hud_add({hud_elem_type = "text", position = {x = 0.02, y = 0.02}, offset = {x = 0, y = 0},
        text = "", alignment = {x = 1, y = 1}, number = 0xFFFFFF})
      player:get_meta():set_int("ai_bridge_hud", id)
    end
    player:hud_change(id, "text", hud_text())
  end
end

local transport_name = {
  healthy = "Healthy", timeout = "Timeout", rejected = "Rejected", disconnected = "Disconnected",
}

local function trunc_toward_zero(value)
  return value < 0 and math.ceil(value) or math.floor(value)
end

local function sense(self, dt)
  local senses, pos = empty_senses(), self.object:get_pos()
  local velocity, yaw = self.object:get_velocity(), self.object:get_yaw()
  local v2_angle = yaw + math.pi / 2
  local eye = vector.add(pos, {x = 0, y = 1, z = 0})
  for index = 0, 15 do
    local angle = v2_angle + index * (2 * math.pi / 16)
    local direction = {x = math.cos(angle), y = 0, z = math.sin(angle)}
    local hit
    for pointed in minetest.raycast(eye, vector.add(eye, vector.multiply(direction, 6)), false, false) do
      if pointed.type == "node" then hit = pointed break end
    end
    local sample = senses.retinal[index + 1]
    if hit then
      local rgb = palette[minetest.get_node(hit.under).name] or {120, 170, 255}
      local light = minetest.get_node_light(hit.above, 0.5) or 0
      local delta = vector.subtract(hit.intersection_point, eye)
      local distance = vector.length(delta)
      sample.red, sample.green, sample.blue = rgb[1], rgb[2], rgb[3]
      sample.brightness = byte(light * 17)
      sample.occlusion = byte(255 * (1 - clamp(distance, 0, 6) / 6))
    end
    sample.motion = signed((velocity.x * direction.x + velocity.z * direction.z) * 255 / 4)
  end
  senses.touch = byte(state.touch)
  senses.joint_position = byte((((v2_angle % (2 * math.pi)) * 256) / (2 * math.pi)))
  senses.orientation = senses.joint_position
  senses.muscle_tension = byte((math.abs(velocity.x) + math.abs(velocity.z)) * 255 / 4)
  local forward = {x = math.cos(v2_angle), z = math.sin(v2_angle)}
  senses.acceleration = signed((((velocity.x - state.previous_velocity.x) * forward.x +
    (velocity.z - state.previous_velocity.z) * forward.z) / math.max(dt, 0.0001)) * 255 / 4)
  state.previous_velocity = {x = velocity.x, z = velocity.z}
  return senses
end

local function send_transition(self, dt)
  if not http then zero_action("disconnected") return end
  local observation = { sequence = state.sequence, senses = sense(self, dt),
    capabilities = { checkpoint_resume = false, dead = { chemicals = {true,true,true,true}, vibration = true,
      touch = false, joint_position = false, muscle_tension = false, acceleration = false, orientation = false } },
    transport = transport_name[state.status] or "Disconnected" }
  local body = minetest.write_json({ protocol = protocol, agent_id = agent_id, observation = observation,
    trace = { applied_server_step = state.applied_step, applied_steps = state.applied_steps,
      accumulated_dtime_us = math.floor(state.elapsed * 1000000 + 0.5), applied_action = state.held, pose = pose(self) } })
  local ray = observation.senses.retinal[1]
  smoke_log({event = "observation", sequence = state.sequence, transport = observation.transport, touch = observation.senses.touch,
    ray0 = {red = ray.red, green = ray.green, blue = ray.blue, occlusion = ray.occlusion}})
  state.pending, state.pending_at, state.request_token = true, minetest.get_us_time(), state.request_token + 1
  local token = state.request_token
  http.fetch({url = endpoint, post_data = body, extra_headers = {["Content-Type"] = "application/json"}}, function(result)
    if token ~= state.request_token then return end
    state.pending = false
    local reply = minetest.parse_json(result.data or "")
    if not result.succeeded then
      if reply and type(reply.expected_sequence) == "number" then
        state.sequence = reply.expected_sequence
        zero_action("rejected")
      else
        zero_action("disconnected")
      end
      return
    end
    if not reply or reply.protocol ~= protocol or reply.agent_id ~= agent_id
        or reply.generated_sequence ~= state.sequence or reply.expected_sequence ~= state.sequence + 1 then
      zero_action("rejected") return
    end
    state.held, state.status = reply.action, "healthy"
    state.diagnostics = reply.diagnostics
    state.latency_ms = math.floor((minetest.get_us_time() - state.pending_at) / 1000 + 0.5)
    if scripted then
      -- The bridge remains live; this deterministic actuator fixture makes collision and yaw
      -- evidence independent of the founder's learned motor output.
      if state.sequence < 5 then state.held = { thrust = 0, turn = 255, jaw = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} }
      else state.held = { thrust = 255, turn = 0, jaw = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} } end
      smoke_log({event = "override_action", sequence = state.sequence, thrust = state.held.thrust, turn = state.held.turn})
    end
    state.sequence = state.sequence + 1
    state.applied_step, state.applied_steps, state.elapsed = state.server_step, 0, 0
    update_observer_hud()
  end)
end

minetest.register_entity("ai_bridge:founder", {
  initial_properties = {physical = true, collide_with_objects = false, collisionbox = {-0.3,0,-0.3,0.3,1.7,0.3},
    visual = "cube", textures = {"ai_bridge_base.png^[colorize:#487a55:255", "ai_bridge_base.png^[colorize:#487a55:255",
      "ai_bridge_base.png^[colorize:#5a9364:255", "ai_bridge_base.png^[colorize:#31553b:255",
      "ai_bridge_base.png^[colorize:#f0b44c:255",
      "ai_bridge_base.png^[colorize:#487a55:255"}, visual_size = {x = 1.35, y = 1.35},
    glow = 8, nametag = "FOUNDER — Rust bridge controlled", nametag_color = "#ffe08a",
    static_save = false},
  on_activate = function(self)
    -- V2 heading zero is +X while Luanti yaw -pi/2 faces +X.
    self.object:set_yaw(-math.pi / 2)
    state.founder = self.object
  end,
  on_step = function(self, dtime, moveresult)
    state.server_step, state.applied_steps, state.elapsed = state.server_step + 1, state.applied_steps + 1, state.elapsed + dtime
    state.touch = 0
    if state.smoke and state.server_step == 1 then
      local pos, velocity = self.object:get_pos(), self.object:get_velocity()
      smoke_log({event = "spawn", x = pos.x, y = pos.y, z = pos.z, yaw = self.object:get_yaw(), vy = velocity.y})
    elseif state.smoke and state.server_step == 2 then
      smoke_log({event = "gravity", vy = self.object:get_velocity().y})
    end
    if moveresult and moveresult.collisions then
      for _, collision in ipairs(moveresult.collisions) do
        if collision.axis == "x" or collision.axis == "z" then state.touch = 255 break end
      end
    end
    if state.pending and (minetest.get_us_time() - state.pending_at) / 1000000 > request_timeout then
      state.request_token = state.request_token + 1 -- late reply cannot replace the safety action
      state.pending = false
      zero_action("timeout")
      update_observer_hud()
    end
    local action, yaw = state.held, self.object:get_yaw()
    local v2_angle = yaw + math.pi / 2
    local forward = {x = math.cos(v2_angle), z = math.sin(v2_angle)}
    local velocity = self.object:get_velocity()
    local target = clamp((action.thrust or 0) * 4 / 256, -4, 4)
    local current = velocity.x * forward.x + velocity.z * forward.z
    local control = clamp((target - current) / math.max(dtime, 0.0001), -8, 8)
    self.object:set_acceleration({x = forward.x * control, y = -9.81, z = forward.z * control})
    if state.applied_steps == 1 then
      local turn = clamp(trunc_toward_zero((action.turn or 0) / 32), -8, 8)
      self.object:set_yaw(yaw + turn * (2 * math.pi / 256))
    end
    if not state.pending and state.elapsed >= step_seconds then send_transition(self, state.elapsed) end
  end,
})

minetest.register_on_mods_loaded(function()
  minetest.after(0, function()
    minetest.emerge_area({x=-7,y=0,z=-7}, {x=7,y=4,z=7}, function(_, _, remaining)
      if remaining ~= 0 then return end
      minetest.set_timeofday(0.5)
      -- Luanti yaw -pi/2 maps V2 heading 0 to +X; positive turns move ray 0 toward +Z.
      for x = -4, 4 do for z = -4, 4 do
        minetest.set_node({x=x,y=0,z=z}, {name="ai_bridge:floor"})
        minetest.set_node({x=x,y=4,z=z}, {name="ai_bridge:observer_glass"})
      end end
      for y = 1, 3 do for axis = -4, 4 do
        if axis == -4 or axis == 4 then
          for other = -4, 4 do
            minetest.set_node({x=axis,y=y,z=other}, {name="ai_bridge:wall_x"})
            minetest.set_node({x=other,y=y,z=axis}, {name="ai_bridge:wall_z"})
          end
        end
      end end
      -- These fixtures illuminate every face from the authoritative server map. Their height
      -- keeps them out of the founder's y=1 horizontal receptor rays.
      for _, light in ipairs({
        {x=-3, y=3, z=-3}, {x=-3, y=3, z=3},
        {x=3, y=3, z=-3}, {x=3, y=3, z=3},
      }) do
        minetest.set_node(light, {name="ai_bridge:observer_light"})
      end
      minetest.fix_light({x=-5, y=-1, z=-5}, {x=5, y=5, z=5})
      if state.smoke then
        local floor = minetest.get_node({x=0, y=0, z=0})
        local wall = minetest.get_node({x=4, y=2, z=0})
        smoke_log({event = "room", floor = floor.name, wall = wall.name,
          floor_light = minetest.get_node_light({x=0, y=0, z=0}, 0.5) or -1,
          wall_light = minetest.get_node_light({x=4, y=2, z=0}, 0.5) or -1})
      end
      minetest.add_entity({x=0,y=1,z=0}, "ai_bridge:founder")
      if mode == "smoke" then
        minetest.after(7, function() minetest.request_shutdown("ai_bridge smoke completed", true) end)
      end
    end)
  end)
end)

minetest.register_on_joinplayer(function(player)
  if player:get_player_name() ~= observer_name then return end
  -- The observer is a normal, non-privileged explorer. Its controls affect only the player:
  -- the founder remains a separate entity whose actuators are driven only by Rust bridge replies.
  -- Start inside the lit room so the initial framing keeps the founder and its label readable.
  minetest.set_player_privs(observer_name, {})
  player:set_physics_override({speed = 1, jump = 1, gravity = 1})
  player:set_pos({x = 0, y = 2, z = -3})
  player:set_look_horizontal(0)
  player:set_look_vertical(-0.55)
  update_observer_hud()
end)

minetest.register_chatcommand("founder_reset", {
  description = "Observer-only: return the founder to the arena centre without changing its brain or action.",
  func = function(name)
    if name ~= observer_name then return false, "Only the configured observer may reset the founder." end
    local founder = state.founder
    if not founder or not founder:get_pos() then return false, "The founder is not available yet." end
    -- This changes only scene placement. The held Rust action, next observation, sequence, and
    -- runtime state remain untouched: it is intentionally not collision avoidance or steering.
    founder:move_to({x = 0, y = 1, z = 0}, false)
    founder:set_velocity({x = 0, y = 0, z = 0})
    founder:set_acceleration({x = 0, y = -9.81, z = 0})
    founder:set_yaw(-math.pi / 2)
    state.previous_velocity, state.touch = {x = 0, z = 0}, 0
    state.observer_resets = state.observer_resets + 1
    update_observer_hud()
    return true, "Founder returned to centre. This is an observer scene reset; Rust action and learning were not changed."
  end,
})

minetest.register_globalstep(function()
  update_observer_hud()
end)
