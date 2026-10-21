-- One physical founder and an intentionally narrow, loopback-only bridge protocol.
-- No coordinates, node names, recipes, rewards, or world facts cross into Rust.
local palette = dofile(minetest.get_modpath("ai_bridge") .. "/palette_generated.lua").colours
local protocol, agent_id = 3, 1
local step_seconds, request_timeout = 0.1, 0.5
local endpoint = minetest.settings:get("ai_bridge_endpoint") or "http://127.0.0.1:8481/v1/transition"
local http = minetest.request_http_api()
local mode = minetest.settings:get("ai_bridge_mode") or "observe"
if mode ~= "observe" and mode ~= "smoke" and mode ~= "demo-motion" and mode ~= "learn" then
  minetest.log("error", "ai_bridge: invalid ai_bridge_mode; using observe")
  mode = "observe"
end
local scripted = mode == "smoke" or mode == "demo-motion"
-- Learning runs follow the infant mobile paradigm: baseline without the object, a connected
-- phase with it in front of the founder, then extinction without it. In the "inert" control the
-- object is present and solid but a hand pulse neither moves it nor reports contact. Every trial
-- the experimenter returns the founder and object to their places, as one repositions an infant;
-- this changes only the scene, never the held Rust action, sequence, or learning state.
local learn = mode == "learn"
local learn_phases = {}
for count in string.gmatch(minetest.settings:get("ai_bridge_learn_phases") or "200,400,200", "%d+") do
  learn_phases[#learn_phases + 1] = tonumber(count)
end
local learn_trial = tonumber(minetest.settings:get("ai_bridge_learn_trial") or "") or 40
local learn_inert = minetest.settings:get("ai_bridge_learn_object") == "inert"
local observer_name = minetest.settings:get("ai_bridge_observer") or "observer"
-- This is Minetest Game's native player mesh, whose player_api definition uses unit scale.
local founder_visual_scale = 1
local founder_animations = {
  idle = {x = 0, y = 79, speed = 30},
  walk = {x = 168, y = 187, speed = 30},
  hand = {x = 189, y = 198, speed = 30},
}
-- Luanti nodes occupy y +/-0.5 around their integer coordinate.  These positions put
-- collisionbox bottoms directly on the floor node's upper face (y=0.5).
local founder_spawn = {x = 0, y = 0.5, z = 0}
local generic_spawn = {x = 1.2, y = 0.7, z = 0}
local generic_visual_scale = 0.4
-- The physical player mesh occupies x/z +/-0.3 and y 0..1.7 from its position.
-- The inner room is bounded by the faces at +/-4 and y=0/4; the second shell is
-- deliberately outside it, so the physics engine always has a solid neighbour even
-- during maximum thrust, strafe, and jump impulses.
local arena_safety_limit = {horizontal = 5.25, low_y = -0.5, high_y = 5.25}

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
  held = { thrust = 0, turn = 0, strafe = 0, look_pitch = 0, jump = 0, jaw = 0, hand_pulse = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} },
  applied_step = 0, applied_steps = 0, elapsed = 0, previous_velocity = {x = 0, z = 0}, touch = 0, grounded = 0, hand_contact = 0, eye_pitch = 0, status = "healthy",
  smoke = mode == "smoke", learn_decisions = 0, learn_phase = nil, learn_cube = nil, diagnostics = nil, latency_ms = 0, founder = nil, body_parts = {},
  observer_resets = 0, safety_resets = 0, last_safety_reset = "none" }

local function smoke_log(event)
  if state.smoke then minetest.log("action", "AI_BRIDGE_SMOKE " .. minetest.write_json(event)) end
end

local function zero_action(reason)
  state.held = { thrust = 0, turn = 0, strafe = 0, look_pitch = 0, jump = 0, jaw = 0, hand_pulse = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} }
  state.status = reason
  smoke_log({event = "safe_zero", status = reason, thrust = state.held.thrust, turn = state.held.turn})
end

local function pose(self)
  local position = self.object:get_pos()
  return {x_millinode = math.floor(position.x * 1000 + 0.5), y_millinode = math.floor(position.y * 1000 + 0.5),
    z_millinode = math.floor(position.z * 1000 + 0.5), yaw_milliradian = math.floor(self.object:get_yaw() * 1000 + 0.5)}
end

local transport_name = {
  healthy = "Healthy", timeout = "Timeout", rejected = "Rejected", disconnected = "Disconnected",
}

-- Observer HUD elements of the current connection, by key. A reconnecting client starts with no
-- HUD, so the table is cleared on join.
local hud = {}

local function hud_set(player, key, definition, field, value)
  if not hud[key] then
    hud[key] = player:hud_add(definition)
  elseif value ~= nil then
    player:hud_change(hud[key], field, value)
  end
end

local function hud_text(player, key, corner, row, text, colour)
  local right = corner == "right"
  hud_set(player, key, {
    type = "text", position = {x = right and 1 or 0, y = 0},
    offset = {x = right and -32 or 32, y = 92 + row * 30},
    alignment = {x = right and -1 or 1, y = 1}, text = text,
    number = colour or 0xFFFFFF, size = {x = 1, y = 0}, z_index = 1,
  }, "text", text)
end

local function hud_panel(player, key, corner, rows)
  local right = corner == "right"
  hud_set(player, key, {
    type = "image", position = {x = right and 1 or 0, y = 0},
    offset = {x = right and -16 or 16, y = 80}, alignment = {x = right and -1 or 1, y = 1},
    text = "ai_bridge_base.png^[colorize:#101418:255^[opacity:185",
    scale = {x = right and -21 or -27, y = -(1.2 + rows * 2.8)}, z_index = 0,
  })
end

local function describe(action)
  local parts = {}
  local function add(condition, words) if condition then parts[#parts + 1] = words end end
  add((action.thrust or 0) > 0, "walking forward")
  add((action.thrust or 0) < 0, "walking back")
  add((action.turn or 0) > 0, "turning left")
  add((action.turn or 0) < 0, "turning right")
  add((action.strafe or 0) ~= 0, "stepping sideways")
  add((action.look_pitch or 0) > 0, "looking up")
  add((action.look_pitch or 0) < 0, "looking down")
  add((action.jump or 0) > 0, "jumping")
  add((action.hand_pulse or 0) > 0, "pushing with hand")
  add((action.jaw or 0) > 0, "opening jaw")
  add((action.acoustic or 0) > 0, "making a sound")
  return #parts > 0 and table.concat(parts, ", ") or "still"
end

local function update_observer_hud()
  local player = minetest.get_player_by_name(observer_name)
  if not player then return end
  local diagnostics = state.diagnostics or {}
  local curious = diagnostics.experiment_mode == "AffordanceDiscovery"
  local lines = {
    {"AI VILLAGE  |  OBSERVER", 0xFFE08A},
    {"Founder 1  |  " .. (curious and "curious explorer" or "baseline brain")
      .. (scripted and "  |  test fixture driving" or ""), 0xD8E2EA},
    {"Doing:  " .. describe(state.held or {})
      .. (curious and ("   (interest " .. tostring(diagnostics.intrinsic_score or 0) .. ")") or ""), 0xFFFFFF},
    {"Body:  " .. (state.grounded > 0 and "on the ground" or "in the air")
      .. "  |  hand contact: " .. (state.hand_contact > 0 and "YES" or "no"), 0xFFFFFF},
    {"Link:  " .. string.lower(transport_name[state.status]) .. "  |  " .. tostring(state.latency_ms)
      .. " ms  |  decision " .. tostring(diagnostics.decision_tick or "—"), 0xB8C4CE},
    {"Resets:  observer " .. tostring(state.observer_resets) .. "  |  containment "
      .. tostring(state.safety_resets), 0xB8C4CE},
  }
  hud_panel(player, "left_panel", "left", #lines)
  for row, line in ipairs(lines) do
    hud_text(player, "left_" .. row, "left", row - 1, line[1], line[2])
  end
end

local function trunc_toward_zero(value)
  return value < 0 and math.ceil(value) or math.floor(value)
end

-- Ray-visible physical entities and their average texture colour. Only these entities stop eye
-- rays; players (observers) and the founder's own body never do. Nothing names the entity to
-- Rust: a ray reports only colour, light, distance, and apparent motion.
local entity_colours = {
  ["ai_bridge:test_object"] = {176, 176, 176}, -- average of ai_bridge_generic_cc0.png
}

local function sense(self, dt)
  local senses, pos = empty_senses(), self.object:get_pos()
  local velocity, yaw = self.object:get_velocity(), self.object:get_yaw()
  local v2_angle = yaw + math.pi / 2
  local pitch = state.eye_pitch * math.pi / 512
  local eye = vector.add(pos, {x = 0, y = 1, z = 0})
  for index = 0, 15 do
    local angle = v2_angle + index * (2 * math.pi / 16)
    local direction = {x = math.cos(angle) * math.cos(pitch), y = math.sin(pitch), z = math.sin(angle) * math.cos(pitch)}
    local hit, rgb, light, hit_velocity
    for pointed in minetest.raycast(eye, vector.add(eye, vector.multiply(direction, 6)), true, false) do
      if pointed.type == "node" then
        hit, rgb = pointed, palette[minetest.get_node(pointed.under).name] or {120, 170, 255}
        light = minetest.get_node_light(pointed.above, 0.5) or 0
        break
      end
      local entity = pointed.ref ~= self.object and pointed.ref:get_luaentity()
      local colour = entity and entity_colours[entity.name]
      if colour then
        hit, rgb, hit_velocity = pointed, colour, pointed.ref:get_velocity()
        light = minetest.get_node_light(vector.round(pointed.ref:get_pos()), 0.5) or 0
        break
      end
    end
    local sample = senses.retinal[index + 1]
    if hit then
      local delta = vector.subtract(hit.intersection_point, eye)
      local distance = vector.length(delta)
      sample.red, sample.green, sample.blue = rgb[1], rgb[2], rgb[3]
      sample.brightness = byte(light * 17)
      sample.occlusion = byte(255 * (1 - clamp(distance, 0, 6) / 6))
    end
    -- Apparent closing speed along the ray: own motion, minus the motion of a seen object.
    local seen = hit_velocity or {x = 0, z = 0}
    sample.motion = signed(((velocity.x - seen.x) * direction.x + (velocity.z - seen.z) * direction.z) * 255 / 4)
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

local function learn_log(event)
  minetest.log("action", "AI_BRIDGE_LEARN " .. minetest.write_json(event))
end

local function learn_phase_name(decision)
  local names, start = {"baseline", "connected", "extinction"}, 0
  for index, count in ipairs(learn_phases) do
    if decision <= start + count then return names[index] or "done" end
    start = start + count
  end
  return "done"
end

-- Called once per accepted Rust reply, before the reply's action is applied.
local function learn_step()
  state.learn_decisions = state.learn_decisions + 1
  local decision, founder = state.learn_decisions, state.founder
  local phase = learn_phase_name(decision)
  if phase ~= state.learn_phase then
    state.learn_phase = phase
    learn_log({event = "phase", phase = phase, decision = decision, sequence = state.sequence,
      object = learn_inert and "inert" or "free"})
    if phase == "done" then
      minetest.request_shutdown("ai_bridge learning run completed", false)
      return
    end
  end
  local cube = state.learn_cube
  if cube and not cube:get_pos() then cube, state.learn_cube = nil, nil end
  local want_cube = phase == "connected"
  if want_cube and not cube then
    state.learn_cube = minetest.add_entity(generic_spawn, "ai_bridge:test_object")
  elseif not want_cube and cube then
    cube:remove()
    state.learn_cube = nil
  end
  if (decision - 1) % learn_trial == 0 and founder and founder:get_pos() then
    founder:move_to(founder_spawn, false)
    founder:set_velocity({x = 0, y = 0, z = 0})
    founder:set_yaw(-math.pi / 2)
    state.eye_pitch, state.previous_velocity, state.touch = 0, {x = 0, z = 0}, 0
    if state.learn_cube then
      state.learn_cube:move_to(generic_spawn, false)
      state.learn_cube:set_velocity({x = 0, y = 0, z = 0})
    end
    learn_log({event = "trial", decision = decision, sequence = state.sequence})
  end
end

local function send_transition(self, dt)
  if not http then zero_action("disconnected") return end
  local velocity = self.object:get_velocity()
  local observation = { sequence = state.sequence, senses = sense(self, dt),
    body = { eye_pitch = byte(state.eye_pitch + 128), grounded = state.grounded, vertical_motion = signed(velocity.y * 64), hand_contact = state.hand_contact },
    capabilities = { body_contract_version = 3, checkpoint_resume = false, dead = { chemicals = {true,true,true,true}, vibration = true,
      touch = false, joint_position = false, muscle_tension = false, acceleration = false, orientation = false } },
    transport = transport_name[state.status] or "Disconnected" }
  local body = minetest.write_json({ protocol = protocol, agent_id = agent_id, observation = observation,
    trace = { applied_server_step = state.applied_step, applied_steps = state.applied_steps,
      accumulated_dtime_us = math.floor(state.elapsed * 1000000 + 0.5), applied_action = state.held, pose = pose(self) } })
  -- Hand contact is a bounded event latch: preserve it until the one subsequent observation,
  -- then clear it.  It never carries target identity or any other world fact.
  state.hand_contact = 0
  state.last_retinal = observation.senses.retinal
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
      local sequence = state.sequence
      state.held = { thrust = 0, turn = 0, strafe = 0, look_pitch = 0, jump = 0,
        hand_pulse = 0, jaw = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} }
      if sequence == 0 then state.held.hand_pulse = 255
      elseif sequence < 8 then state.held.turn = 255
      elseif sequence < 55 then state.held.thrust, state.held.jump = 255, 255
      elseif sequence < 102 then state.held.strafe, state.held.jump = -255, 255
      elseif sequence < 149 then state.held.strafe, state.held.jump = 255, 255
      else state.held.thrust, state.held.hand_pulse = -255, 255 end
      smoke_log({event = "override_action", sequence = sequence, thrust = state.held.thrust,
        strafe = state.held.strafe, jump = state.held.jump, turn = state.held.turn,
        hand_pulse = state.held.hand_pulse})
    end
    if learn then learn_step() end
    state.sequence = state.sequence + 1
    state.applied_step, state.applied_steps, state.elapsed = state.server_step, 0, 0
    update_observer_hud()
  end)
end

-- Keep the neutral physics fixture's material a direct six-face media reference.
local function cube_textures(texture)
  return {texture, texture, texture, texture, texture, texture}
end

local function update_humanoid_pose(object, action)
  -- This mesh can show one native animation at a time.  Locomotion therefore wins over
  -- the mine frame: a nonzero movement command (or residual physical movement) is always
  -- visibly walking, while hand is shown only for a nonzero pulse while stationary.
  local name = "idle"
  local velocity = object:get_velocity()
  local moving = math.sqrt(velocity.x * velocity.x + velocity.z * velocity.z) > 0.08
  if (action.thrust or 0) ~= 0 or (action.strafe or 0) ~= 0 or moving then
    name = "walk"
  elseif (action.hand_pulse or 0) > 0 then
    name = "hand"
  end
  if state.founder_animation == name then return end
  local animation = founder_animations[name]
  object:set_animation({x = animation.x, y = animation.y}, animation.speed, 0, true)
  state.founder_animation = name
  minetest.log("action", "AI_BRIDGE_FOUNDER_ANIMATION name=" .. name
    .. " frames=" .. animation.x .. "-" .. animation.y
    .. " thrust=" .. tostring(action.thrust or 0)
    .. " strafe=" .. tostring(action.strafe or 0)
    .. " hand=" .. tostring(action.hand_pulse or 0))
  smoke_log({event = "animation", name = name, thrust = action.thrust or 0,
    strafe = action.strafe or 0, hand_pulse = action.hand_pulse or 0})
end

minetest.register_entity("ai_bridge:founder", {
  initial_properties = {physical = true, collide_with_objects = false, collisionbox = {-0.3,0,-0.3,0.3,1.7,0.3},
    visual = "mesh", mesh = "minetest_game_character.b3d",
    textures = {"minetest_game_character.png"}, visual_size = {x = founder_visual_scale, y = founder_visual_scale},
    nametag = "FOUNDER — Rust-controlled humanoid", nametag_color = "#ffe08a",
    static_save = false},
  on_activate = function(self)
    -- V2 heading zero is +X while Luanti yaw -pi/2 faces +X.
    self.object:set_yaw(-math.pi / 2)
    self.label = "Founder " .. tostring(agent_id)
    state.founder = self.object
    state.founder_animation = nil
    update_humanoid_pose(self.object, state.held)
    local properties = self.object:get_properties()
    minetest.log("action", "AI_BRIDGE_FOUNDER_VISUAL mesh=" .. tostring(properties.mesh)
      .. " texture=" .. tostring(properties.textures and properties.textures[1]))
  end,
  on_step = function(self, dtime, moveresult)
    state.server_step, state.applied_steps, state.elapsed = state.server_step + 1, state.applied_steps + 1, state.elapsed + dtime
    state.touch, state.grounded = 0, 0
    if state.smoke and state.server_step == 1 then
      local pos, velocity = self.object:get_pos(), self.object:get_velocity()
      smoke_log({event = "spawn", x = pos.x, y = pos.y, z = pos.z, yaw = self.object:get_yaw(), vy = velocity.y})
    elseif state.smoke and state.server_step == 2 then
      smoke_log({event = "gravity", vy = self.object:get_velocity().y})
    end
    if moveresult and moveresult.collisions then
      for _, collision in ipairs(moveresult.collisions) do
        if collision.axis == "x" or collision.axis == "z" then state.touch = 255 end
        if collision.axis == "y" and collision.old_velocity.y < 0 then state.grounded = 255 end
      end
    end
    if state.pending and (minetest.get_us_time() - state.pending_at) / 1000000 > request_timeout then
      state.request_token = state.request_token + 1 -- late reply cannot replace the safety action
      state.pending = false
      zero_action("timeout")
      update_observer_hud()
    end
    local position = self.object:get_pos()
    if math.abs(position.x) > arena_safety_limit.horizontal or math.abs(position.z) > arena_safety_limit.horizontal
        or position.y < arena_safety_limit.low_y or position.y > arena_safety_limit.high_y then
      -- This recovery is intentionally outside the actuator/learning path.  It records a
      -- physics containment failure but preserves held action, sequence, and Rust state.
      local reason = string.format("out_of_bounds x=%.3f y=%.3f z=%.3f", position.x, position.y, position.z)
      minetest.log("warning", "AI_BRIDGE_SAFETY_RESET " .. reason)
      smoke_log({event = "safety_reset", reason = reason, x = position.x, y = position.y, z = position.z})
      self.object:move_to(founder_spawn, false)
      self.object:set_velocity({x = 0, y = 0, z = 0})
      self.object:set_acceleration({x = 0, y = -9.81, z = 0})
      self.object:set_yaw(-math.pi / 2)
      state.previous_velocity, state.touch, state.grounded = {x = 0, z = 0}, 0, 255
      state.safety_resets, state.last_safety_reset = state.safety_resets + 1, reason
      update_observer_hud()
      return
    end
    local action, yaw = state.held, self.object:get_yaw()
    update_humanoid_pose(self.object, action)
    local v2_angle = yaw + math.pi / 2
    local forward = {x = math.cos(v2_angle), z = math.sin(v2_angle)}
    local right = {x = -forward.z, z = forward.x}
    local velocity = self.object:get_velocity()
    local target = clamp((action.thrust or 0) * 4 / 256, -4, 4)
    local current = velocity.x * forward.x + velocity.z * forward.z
    local control = clamp((target - current) / math.max(dtime, 0.0001), -8, 8)
    local side_target = clamp((action.strafe or 0) * 4 / 256, -4, 4)
    local side_current = velocity.x * right.x + velocity.z * right.z
    local side_control = clamp((side_target - side_current) / math.max(dtime, 0.0001), -8, 8)
    self.object:set_acceleration({x = forward.x * control + right.x * side_control, y = -9.81, z = forward.z * control + right.z * side_control})
    if state.applied_steps == 1 then
      local turn = clamp(trunc_toward_zero((action.turn or 0) / 32), -8, 8)
      self.object:set_yaw(yaw + turn * (2 * math.pi / 256))
      state.eye_pitch = clamp(state.eye_pitch + trunc_toward_zero((action.look_pitch or 0) / 32), -128, 127)
      if (action.jump or 0) > 0 and state.grounded > 0 then self.object:set_velocity({x = velocity.x, y = 5, z = velocity.z}) end
      if (action.hand_pulse or 0) > 0 then
        local origin = vector.add(self.object:get_pos(), {x = forward.x * 0.8, y = 0.8, z = forward.z * 0.8})
        -- Deliberately bounded reach around the visible forward hand; this is contact feedback,
        -- not a proximity sensor or a way to identify arbitrary world objects.
        for _, object in ipairs(minetest.get_objects_inside_radius(origin, 1.2)) do
          local entity = object:get_luaentity()
          if entity and entity.name == "ai_bridge:test_object" and not learn_inert then
            object:add_velocity({x = forward.x * 3, y = 1, z = forward.z * 3}); state.hand_contact = 255
            smoke_log({event = "hand_contact", impulse = 3}); break
          end
        end
      end
    end
    if not state.pending and state.elapsed >= step_seconds then send_transition(self, state.elapsed) end
  end,
})

-- The sole hand-pulse target is a physical, nonsemantic test object. It is not sensed by name
-- or exposed to Rust; contact is reported only as a bounded body outcome.
minetest.register_entity("ai_bridge:test_object", {
  initial_properties = {physical = true, collide_with_objects = true, collisionbox = {-0.2,-0.2,-0.2,0.2,0.2,0.2},
    visual = "cube", visual_size = {x = generic_visual_scale, y = generic_visual_scale},
    textures = cube_textures("ai_bridge_generic_cc0.png"), static_save = false},
})

minetest.register_on_mods_loaded(function()
  minetest.after(0, function()
    minetest.emerge_area({x=-7,y=0,z=-7}, {x=7,y=4,z=7}, function(_, _, remaining)
      if remaining ~= 0 then return end
      minetest.set_timeofday(0.5)
      -- Luanti yaw -pi/2 maps V2 heading 0 to +X; positive turns move ray 0 toward +Z.
      for x = -5, 5 do for z = -5, 5 do
        minetest.set_node({x=x,y=-1,z=z}, {name="ai_bridge:floor"})
        minetest.set_node({x=x,y=0,z=z}, {name="ai_bridge:floor"})
        minetest.set_node({x=x,y=4,z=z}, {name="ai_bridge:observer_glass"})
        minetest.set_node({x=x,y=5,z=z}, {name="ai_bridge:observer_glass"})
      end end
      for y = -1, 5 do for axis = -5, 5 do
        if axis == -5 or axis == 5 then
          for other = -5, 5 do
            minetest.set_node({x=axis,y=y,z=other}, {name="ai_bridge:wall_x"})
            minetest.set_node({x=other,y=y,z=axis}, {name="ai_bridge:wall_z"})
          end
        end
      end end
      -- Inner collision faces, plus the outer shell above, leave room for the full
      -- 1.7-node collisionbox to jump without a route around a wall edge.
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
      minetest.fix_light({x=-6, y=-2, z=-6}, {x=6, y=6, z=6})
      if state.smoke then
        local floor = minetest.get_node({x=0, y=0, z=0})
        local wall = minetest.get_node({x=4, y=2, z=0})
        smoke_log({event = "room", floor = floor.name, wall = wall.name,
          floor_light = minetest.get_node_light({x=0, y=0, z=0}, 0.5) or -1,
          wall_light = minetest.get_node_light({x=4, y=2, z=0}, 0.5) or -1})
      end
      minetest.add_entity(founder_spawn, "ai_bridge:founder")
      -- The 0.4-node cube's collision centre is exactly 0.2 nodes above the floor.  It is
      -- therefore grounded at creation rather than falling or receiving any startup impulse.
      if learn then
        -- A headless learning run has no player to keep the arena active, and Luanti unloads
        -- idle map blocks after about 29 s, removing the unsaved founder. Keep the arena's
        -- blocks loaded for this session only.
        for _, x in ipairs({-5, 5}) do for _, y in ipairs({-1, 5}) do for _, z in ipairs({-5, 5}) do
          minetest.forceload_block({x = x, y = y, z = z}, true)
        end end end
      else
        minetest.add_entity(generic_spawn, "ai_bridge:test_object")
      end
      smoke_log({event = "generic_spawn", x = generic_spawn.x, y = generic_spawn.y, z = generic_spawn.z,
        visual_size = generic_visual_scale, collision_height = 0.4})
      if mode == "smoke" then
        minetest.after(18, function() minetest.request_shutdown("ai_bridge smoke completed", true) end)
      end
    end)
  end)
end)

-- Observer point of view. Everything here moves only the observer's camera and HUD: it never
-- changes a founder's held action, senses, sequence, or learning.
local pov = {target = nil, rows = 0}
local POV_FORM = "ai_bridge:founders"

local function founders()
  local list = {}
  for _, entity in pairs(minetest.luaentities) do
    if entity.name == "ai_bridge:founder" and entity.object:get_pos() then
      list[#list + 1] = {object = entity.object, label = entity.label or "Founder"}
    end
  end
  table.sort(list, function(a, b) return a.label < b.label end)
  return list
end

local function founder_picker()
  local list = founders()
  local height = 2.4 + #list * 0.9
  local parts = {
    "formspec_version[4]", "size[5.6," .. height .. "]",
    "position[0.985,0.5]", "anchor[1,0.5]",
    "label[0.4,0.5;Founders in this world]",
  }
  for index, founder in ipairs(list) do
    local watching = pov.target == founder.object and "  (watching)" or ""
    parts[#parts + 1] = "button[0.4," .. (0.4 + index * 0.9) .. ";4.8,0.8;pov_" .. index .. ";"
      .. minetest.formspec_escape(founder.label .. watching) .. "]"
  end
  parts[#parts + 1] = "button[0.4," .. (height - 1.1) .. ";4.8,0.8;free_camera;Free camera]"
  return table.concat(parts)
end

local function clear_eye_strip(player)
  for key, id in pairs(hud) do
    if key:sub(1, 4) == "eye_" then
      player:hud_remove(id)
      hud[key] = nil
    end
  end
end

local function leave_pov(player)
  pov.target = nil
  if player then
    local position = player:get_pos()
    player:set_detach()
    if position then player:set_pos(vector.add(position, {x = 0, y = 0.5, z = 0})) end
    clear_eye_strip(player)
  end
end

local function enter_pov(player, founder)
  leave_pov(player)
  pov.target = founder
  -- Attachment offsets are in tenths of a node, in the founder's frame (+z is where it faces).
  -- A player's camera sits 1.625 nodes above its position; lowering it 0.6 puts it at the
  -- founder's sensing eye, 1 node above its feet, where its eye rays start. Moving it 0.4 forward
  -- keeps the founder's own body behind the camera. An attached player must keep observing the
  -- founder, so the body is not hidden.
  player:set_attach(founder, "", {x = 0, y = -6, z = 4}, {x = 0, y = 0, z = 0})
end

-- 16 rays cover 360 degrees; increasing ray index turns left. Shown left to right with the ray
-- straight ahead in the middle and the ray straight behind at the far left.
local EYE_STRIP_ORDER = {8, 7, 6, 5, 4, 3, 2, 1, 0, 15, 14, 13, 12, 11, 10, 9}

local function update_eye_strip(player)
  local retinal = state.last_retinal
  if not retinal then return end
  hud_set(player, "eye_panel", {type = "image", position = {x = 0.5, y = 1},
    offset = {x = 0, y = -24}, alignment = {x = 0, y = -1},
    text = "ai_bridge_base.png^[colorize:#101418:255^[opacity:185",
    scale = {x = 46, y = 7}, z_index = 0})
  hud_set(player, "eye_label", {type = "text", position = {x = 0.5, y = 1},
    offset = {x = 0, y = -96}, alignment = {x = 0, y = 1}, number = 0xFFE08A, z_index = 1,
    text = "WHAT THE FOUNDER SENSES  |  16 eye rays all around, ahead in the middle"})
  for slot, ray in ipairs(EYE_STRIP_ORDER) do
    local sample = retinal[ray + 1]
    local colour = string.format("%02x%02x%02x", sample.red, sample.green, sample.blue)
    local texture = "ai_bridge_base.png^[colorize:#" .. colour .. ":255"
    hud_set(player, "eye_ray" .. slot, {type = "image", position = {x = 0.5, y = 1},
      offset = {x = (slot - 8.5) * 44, y = -36}, scale = {x = 2.5, y = 2.5},
      alignment = {x = 0, y = -1}, text = texture, z_index = 1}, "text", texture)
  end
end

local function update_founder_list(player)
  local list = founders()
  local lines = {{"FOUNDERS IN THIS WORLD", 0xFFE08A}}
  for _, founder in ipairs(list) do
    local watching = pov.target == founder.object
    lines[#lines + 1] = {(watching and "> " or "- ") .. founder.label .. (watching and "  (watching)" or ""),
      watching and 0x9EF0A8 or 0xFFFFFF}
  end
  lines[#lines + 1] = {pov.target and "Press I, then Free camera to leave" or "Press I to watch a founder",
    0xB8C4CE}
  -- Keep a fixed number of rows so a founder leaving clears its line.
  local rows = math.max(#lines, pov.rows or 0)
  pov.rows = rows
  hud_panel(player, "right_panel", "right", #lines)
  for row = 1, rows do
    local line = lines[row] or {"", 0xFFFFFF}
    hud_text(player, "right_" .. row, "right", row - 1, line[1], line[2])
  end
end

local function update_pov(player)
  update_founder_list(player)
  local founder = pov.target
  if not founder then return end
  if not founder:get_pos() then leave_pov(player) return end
  -- Look exactly where the founder's eyes point: its yaw, and its eye pitch (positive is up).
  player:set_look_horizontal(founder:get_yaw())
  player:set_look_vertical(-state.eye_pitch * math.pi / 512)
  update_eye_strip(player)
end

minetest.register_on_player_receive_fields(function(player, formname, fields)
  if player:get_player_name() ~= observer_name then return end
  if formname ~= POV_FORM and formname ~= "" then return end
  if fields.free_camera then
    leave_pov(player)
  else
    for index, founder in ipairs(founders()) do
      if fields["pov_" .. index] then enter_pov(player, founder.object) end
    end
  end
  player:set_inventory_formspec(founder_picker())
  if formname == POV_FORM and not fields.quit then
    minetest.show_formspec(observer_name, POV_FORM, founder_picker())
  end
end)

minetest.register_chatcommand("founders", {
  description = "Observer-only: choose a founder to watch from its point of view.",
  func = function(name)
    if name ~= observer_name then return false, "Only the configured observer may watch founders." end
    minetest.show_formspec(name, POV_FORM, founder_picker())
    return true
  end,
})

minetest.register_on_leaveplayer(function(player)
  if player:get_player_name() == observer_name then
    leave_pov(nil)
    hud, pov.rows = {}, 0
  end
end)

minetest.register_on_joinplayer(function(player)
  if player:get_player_name() ~= observer_name then return end
  hud, pov.rows = {}, 0
  player:set_inventory_formspec(founder_picker())
  -- The observer is a normal, non-privileged explorer. Its controls affect only the player:
  -- the founder remains a separate entity whose actuators are driven only by Rust bridge replies.
  -- Aim at the founder's chest from far enough away to show the complete humanoid and the one
  -- nearby neutral fixture without making either fill the first frame.
  minetest.set_player_privs(observer_name, {})
  player:set_physics_override({speed = 1, jump = 1, gravity = 1})
  player:set_pos({x = 0, y = 2.1, z = -3})
  player:set_look_horizontal(0)
  player:set_look_vertical(-0.28)
  -- A returning observer must not arrive after the live brain has walked the founder away.
  -- This is presentation-only: it does not alter the held action, protocol sequence, or learning.
  if state.founder and state.founder:get_pos() then
    state.founder:move_to(founder_spawn, false)
    state.founder:set_velocity({x = 0, y = 0, z = 0})
    state.founder:set_acceleration({x = 0, y = -9.81, z = 0})
    state.founder:set_yaw(-math.pi / 2)
  end
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
    founder:move_to(founder_spawn, false)
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
  local player = minetest.get_player_by_name(observer_name)
  if player then update_pov(player) end
end)
