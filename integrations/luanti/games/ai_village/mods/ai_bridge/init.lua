-- One physical founder and an intentionally narrow, loopback-only bridge protocol.
-- No coordinates, node names, recipes, rewards, or world facts cross into Rust.
local palette = dofile(minetest.get_modpath("ai_bridge") .. "/palette_generated.lua").colours
local texture_colours = dofile(minetest.get_modpath("ai_bridge") .. "/texture_colours_generated.lua")
-- "room" is the small experimental arena; "open" lives in whatever world the server generates
-- (for example Minetest Game's endless terrain), with no arena built.
local open_world = minetest.settings:get("ai_bridge_world") == "open"
-- Play mode shares a game with a person who plays normally (keeping their inventory and
-- privileges). The founder is born beside them, and /founder switches the person between playing
-- as themselves and observing through the founder's eyes.
local play = open_world and minetest.settings:get_bool("ai_bridge_play", false)
local protocol, agent_id = 3, 1
-- Seconds per decision. Experiments use 0.1; a slower clock makes behaviour easier to watch.
local step_seconds = math.max(0.05, math.min(2,
  tonumber(minetest.settings:get("ai_bridge_step_seconds") or "") or 0.1))
local request_timeout = 0.5
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
-- With "turned" facing, each trial starts the founder facing one of eight directions in a fixed
-- order instead of toward the cube, so pushing works only after facing it (the reaching test).
local learn_turned = minetest.settings:get("ai_bridge_learn_facing") == "turned"
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
-- In an open world the founder respawns this far below its spawn (a deep cave or the void).
local open_world_fall_limit = 40
local generic_spawn = {x = 1.2, y = 0.7, z = 0}
-- In learning runs and while observing, the cube hangs at the founder's eye height, like a
-- mobile in view: the eye rays start 1 node above the founder's feet (y = 1.5), and the cube
-- has no gravity. After a push it swings back to this place a few decisions later.
local learn_cube_spawn = {x = 1.2, y = 1.5, z = 0}
local learn_cube_return_decisions = 4
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

local state = { pain = 0, sequence = 0, server_step = 0, pending = false, pending_at = 0, request_token = 0,
  held = { thrust = 0, turn = 0, strafe = 0, look_pitch = 0, jump = 0, jaw = 0, hand_pulse = 0, acoustic = 0, chemical = 0, visual = 0, signals = {0,0,0,0,0,0,0} },
  applied_step = 0, applied_steps = 0, elapsed = 0, previous_velocity = {x = 0, z = 0}, touch = 0, grounded = 0, hand_contact = 0, eye_pitch = 0, status = "healthy",
  smoke = mode == "smoke", learn_decisions = 0, learn_phase = nil, learn_cube = nil,
  decisions = 0, pushes = 0, hits = 0, pushed_at = nil, swallowed = 0, meals = 0,
  forceloaded = {}, diagnostics = nil, latency_ms = 0, founder = nil, body_parts = {},
  observer_resets = 0, safety_resets = 0, last_safety_reset = "none" }

-- Automated body tests (integrations/luanti/ci/body-test.sh) may steer the held action and read
-- the bridge state through this table; it exists only when ai_bridge_test_hooks is set.
local test_hooks = minetest.settings:get_bool("ai_bridge_test_hooks", false)
if test_hooks then
  rawset(_G, "ai_bridge_test", {state = state})
end

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
  add((action.use_pulse or 0) > 0, "using what is in reach")
  add((action.next_slot or 0) > 0, "taking the next item in hand")
  return #parts > 0 and table.concat(parts, ", ") or "still"
end

local function update_observer_hud()
  -- In play mode the founder's HUD belongs to observing, not to playing as oneself.
  if play and not state.observing then return end
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
    {(state.pushed_at and state.decisions - state.pushed_at < 3) and ">>> HIT! the push moved the cube <<<"
      or ((state.held or {}).hand_pulse or 0) > 0 and ">>> PUSHING <<<" or " ",
      (state.pushed_at and state.decisions - state.pushed_at < 3) and 0x7CFC8C or 0xFFC857},
    {"Meals:  " .. tostring(state.meals) .. "  |  pushes: " .. tostring(state.pushes) .. "  |  hits: " .. tostring(state.hits)
      .. (state.pushes > 0 and ("  (" .. tostring(math.floor(100 * state.hits / state.pushes + 0.5)) .. "%)") or ""),
      0xFFFFFF},
    {"Link:  " .. string.lower(transport_name[state.status]) .. "  |  " .. tostring(state.latency_ms)
      .. " ms  |  decision " .. tostring(diagnostics.decision_tick or "—"), 0xB8C4CE},
    {"Resets:  observer " .. tostring(state.observer_resets) .. "  |  containment "
      .. tostring(state.safety_resets), 0xB8C4CE},
  }
  if play then
    local founder = state.founder and state.founder:get_pos() and state.founder
    local hunger = rawget(_G, "mcl_hunger")
    table.insert(lines, 5, {("Life:  health %d/20  |  hunger %d/20  |  deaths %d"):format(
      founder and founder:get_hp() or 0, founder and hunger and hunger.get_hunger(founder) or 0,
      state.deaths or 0), 0xFF9A9A})
  end
  hud_panel(player, "left_panel", "left", #lines)
  for row, line in ipairs(lines) do
    hud_text(player, "left_" .. row, "left", row - 1, line[1], line[2])
  end
end

local function trunc_toward_zero(value)
  return value < 0 and math.ceil(value) or math.floor(value)
end

-- Colour of a block as an eye sees it: the mod's palette, else the average colour of its top
-- texture, compositing overlays by how much of them is opaque. Cached per block name.
local node_colours = {}
local function texture_colour(texture)
  local colour
  for part in string.gmatch(texture, "[^%^]+") do
    local file = part:gsub("^%(", ""):gsub("%)$", "")
    local summary = texture_colours[file]
    if summary then
      if not colour then
        colour = {summary[1], summary[2], summary[3]}
      else
        local cover = summary[4] / 255
        for channel = 1, 3 do
          colour[channel] = colour[channel] * (1 - cover) + summary[channel] * cover
        end
      end
    end
  end
  return colour
end
local function node_colour(name)
  local cached = node_colours[name]
  if cached then return cached end
  local colour = palette[name]
  if not colour then
    local def = minetest.registered_nodes[name]
    local tile = def and def.tiles and def.tiles[1]
    if type(tile) == "table" then tile = tile.name or tile.image end
    colour = type(tile) == "string" and texture_colour(tile) or nil
    colour = colour and {math.floor(colour[1] + 0.5), math.floor(colour[2] + 0.5), math.floor(colour[3] + 0.5)}
      or {128, 128, 128}
  end
  node_colours[name] = colour
  return colour
end

-- Whether opening the jaw on a block feeds the founder: the block, or what it yields, is edible
-- in the world's own terms (a "food_*" group). This stays inside the body: Rust only feels how
-- much was swallowed, never what.
local function edible(name)
  local function food(item)
    local def = item and minetest.registered_items[item]
    for group in pairs(def and def.groups or {}) do
      if group:sub(1, 5) == "food_" or group == "eatable" then return true end
    end
    return false
  end
  local def = minetest.registered_nodes[name]
  return food(name) or (def and type(def.drop) == "string" and food(def.drop))
end

-- Eats the first edible block the mouth reaches: rays from the eyes slanting forward and down,
-- about one and a half nodes long. The block is consumed as if picked, so it regrows as the
-- world's own rules say (an apple tree regrows its apple, a bush its berries).
local function eat_in_front(self)
  local pos = self.object:get_pos()
  local angle = self.object:get_yaw() + math.pi / 2
  local eye = vector.add(pos, {x = 0, y = 1, z = 0})
  for _, slant in ipairs({0, math.rad(25), math.rad(50)}) do
    local direction = {x = math.cos(angle) * math.cos(slant), y = -math.sin(slant), z = math.sin(angle) * math.cos(slant)}
    for pointed in minetest.raycast(eye, vector.add(eye, vector.multiply(direction, 1.6)), false, false) do
      if pointed.type == "node" then
        local node = minetest.get_node(pointed.under)
        if edible(node.name) then
          local def = minetest.registered_nodes[node.name]
          local meta = minetest.get_meta(pointed.under):to_table()
          minetest.remove_node(pointed.under)
          if def and def.after_dig_node then def.after_dig_node(pointed.under, node, meta, nil) end
          state.swallowed, state.meals = 255, state.meals + 1
          minetest.log("action", "AI_BRIDGE_MEAL " .. tostring(state.meals))
          return true
        end
        break
      end
    end
  end
  return false
end

-- Ray-visible physical entities and their average texture colour. Only these entities stop eye
-- rays; players (observers) and the founder's own body never do. Nothing names the entity to
-- Rust: a ray reports only colour, light, distance, and apparent motion.
local entity_colours = {
  ["ai_bridge:test_object"] = {176, 176, 176}, -- average of ai_bridge_generic_cc0.png
}

-- Casts one eye ray into `sample`: colour, light and nearness of the first ray-visible node or
-- entity within six nodes, and the apparent closing speed along the ray. `vertical` includes
-- vertical motion in that speed (the forward retina); the legacy rays use horizontal motion only.
local function cast_eye_ray(self, eye, direction, velocity, sample, vertical)
  local hit, rgb, light, hit_velocity
  for pointed in minetest.raycast(eye, vector.add(eye, vector.multiply(direction, 6)), true, open_world) do
    if pointed.type == "node" then
      hit, rgb = pointed, node_colour(minetest.get_node(pointed.under).name)
      light = minetest.get_node_light(pointed.above, 0.5) or 0
      break
    end
    local entity = pointed.ref ~= self.object and pointed.ref:get_luaentity()
    local colour = entity and entity_colours[entity.name]
    -- In a shared game the eyes also meet mobs, dropped items and other players, seen by the
    -- average colour of their skin (never their name). The founder's own body and an observer's
    -- hidden one are not seen.
    if not colour and play and pointed.ref ~= self.player then
      local properties = pointed.ref:get_properties()
      local texture = properties.is_visible ~= false and properties.textures and properties.textures[1]
      local average = type(texture) == "string" and texture ~= "" and texture_colour(texture)
      colour = average and {math.floor(average[1] + 0.5), math.floor(average[2] + 0.5), math.floor(average[3] + 0.5)}
    end
    if colour then
      hit, rgb, hit_velocity = pointed, colour, pointed.ref:get_velocity()
      light = minetest.get_node_light(vector.round(pointed.ref:get_pos()), 0.5) or 0
      break
    end
  end
  if hit then
    local distance = vector.length(vector.subtract(hit.intersection_point, eye))
    sample.red, sample.green, sample.blue = rgb[1], rgb[2], rgb[3]
    sample.brightness = byte(light * 17)
    sample.occlusion = byte(255 * (1 - clamp(distance, 0, 6) / 6))
  end
  -- Apparent closing speed along the ray: own motion, minus the motion of a seen object.
  local seen = hit_velocity or {x = 0, y = 0, z = 0}
  local closing = (velocity.x - seen.x) * direction.x + (velocity.z - seen.z) * direction.z
  if vertical then closing = closing + (velocity.y - (seen.y or 0)) * direction.y end
  sample.motion = signed(closing * 255 / 4)
end

-- The forward retina (body contract version 4): 24 columns 7.5 degrees apart and 5 rows 20
-- degrees apart, turned and tilted with the gaze. Point row * 24 + column looks
-- (column - 11.5) * 7.5 degrees left of the gaze and (row - 2) * 20 degrees above it.
local retina_columns, retina_rows = 24, 5
local function sense_retina(self, eye, v2_angle, pitch, velocity)
  local points = {}
  for row = 0, retina_rows - 1 do
    local elevation = pitch + (row - 2) * math.rad(20)
    for column = 0, retina_columns - 1 do
      local angle = v2_angle + (column - (retina_columns - 1) / 2) * math.rad(7.5)
      local direction = {x = math.cos(angle) * math.cos(elevation), y = math.sin(elevation),
        z = math.sin(angle) * math.cos(elevation)}
      local sample = {red = 120, green = 170, blue = 255, brightness = 204, motion = 0, occlusion = 0}
      cast_eye_ray(self, eye, direction, velocity, sample, true)
      points[#points + 1] = sample
    end
  end
  return points
end

-- The nose (open world only): two nostrils a little ahead of the eyes, 0.3 nodes either side.
-- Every edible block gives off an odour that falls with the square of distance; each nostril
-- reports the summed odour on a compressed (logarithmic) scale, as real smell does. The founder
-- feels only two amplitudes in chemical lobes 0 (left) and 1 (right), never what or where.
local odour_radius = 12
local odour_sources
local function sniff(eye, v2_angle)
  if not odour_sources then
    odour_sources = {}
    for name in pairs(minetest.registered_nodes) do
      if edible(name) then odour_sources[#odour_sources + 1] = name end
    end
  end
  local corner = vector.new(odour_radius, odour_radius, odour_radius)
  local sources = minetest.find_nodes_in_area(vector.subtract(eye, corner), vector.add(eye, corner), odour_sources)
  local amplitudes = {}
  for index, side in ipairs({1, -1}) do
    local nostril = {x = eye.x + 0.3 * math.cos(v2_angle) + side * 0.3 * math.cos(v2_angle + math.pi / 2), y = eye.y,
      z = eye.z + 0.3 * math.sin(v2_angle) + side * 0.3 * math.sin(v2_angle + math.pi / 2)}
    local odour = 0
    for _, source in ipairs(sources) do
      local distance = vector.distance(nostril, source)
      if distance <= odour_radius then odour = odour + 1 / (1 + distance * distance) end
    end
    amplitudes[index] = byte(64 * math.log(1 + 64 * odour) / math.log(2))
  end
  return amplitudes
end

local function sense(self, dt)
  local senses, pos = empty_senses(), self.object:get_pos()
  local velocity, yaw = self.object:get_velocity(), self.object:get_yaw()
  local v2_angle = yaw + math.pi / 2
  local pitch = state.eye_pitch * math.pi / 512
  local eye = vector.add(pos, {x = 0, y = self.eye_height or 1, z = 0})
  for index = 0, 15 do
    local angle = v2_angle + index * (2 * math.pi / 16)
    local direction = {x = math.cos(angle) * math.cos(pitch), y = math.sin(pitch), z = math.sin(angle) * math.cos(pitch)}
    cast_eye_ray(self, eye, direction, velocity, senses.retinal[index + 1], false)
  end
  state.retina = sense_retina(self, eye, v2_angle, pitch, velocity)
  if open_world then
    local odour = sniff(eye, v2_angle)
    senses.chemicals[1], senses.chemicals[2] = odour[1], odour[2]
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

-- Returns a pushed hanging cube to its place once enough decisions have passed.
local function swing_back(cube)
  if cube and cube:get_pos() and state.pushed_at
      and state.decisions - state.pushed_at >= learn_cube_return_decisions then
    cube:move_to(learn_cube_spawn, false)
    cube:set_velocity({x = 0, y = 0, z = 0})
    state.pushed_at = nil
  end
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
    state.learn_cube = minetest.add_entity(learn_cube_spawn, "ai_bridge:test_object")
  elseif not want_cube and cube then
    cube:remove()
    state.learn_cube = nil
  end
  swing_back(state.learn_cube)
  if (decision - 1) % learn_trial == 0 and founder and founder:get_pos() then
    founder:move_to(founder_spawn, false)
    founder:set_velocity({x = 0, y = 0, z = 0})
    local trial = math.floor((decision - 1) / learn_trial)
    local turn = learn_turned and (trial * 3) % 8 or 0
    founder:set_yaw(-math.pi / 2 + turn * math.pi / 4)
    state.eye_pitch, state.previous_velocity, state.touch = 0, {x = 0, z = 0}, 0
    if state.learn_cube then
      state.learn_cube:move_to(learn_cube_spawn, false)
      state.learn_cube:set_velocity({x = 0, y = 0, z = 0})
    end
    learn_log({event = "trial", decision = decision, sequence = state.sequence, turn = turn})
  end
end

-- What a player body feels of itself (body contract version 6), from the game's own health,
-- hunger and breath, and the hurt it took since the previous observation. Never what hurt it.
local function felt_body(player)
  local properties = player:get_properties()
  local hp_max, breath_max = properties.hp_max or 20, properties.breath_max or 10
  local hunger = rawget(_G, "mcl_hunger")
  local fullness = hunger and hunger.get_hunger(player) / 20 or 1
  return {health = byte(player:get_hp() * 255 / hp_max), fullness = byte(fullness * 255),
    air = byte(player:get_breath() * 255 / breath_max), pain = byte((state.pain or 0) * 255 / hp_max)}
end

local function send_transition(self, dt)
  if not http then zero_action("disconnected") return end
  local velocity = self.object:get_velocity()
  -- Sense first: it also fills the forward retina sent beside the legacy rays.
  local senses = sense(self, dt)
  local observation = { sequence = state.sequence, senses = senses,
    body = { eye_pitch = byte(state.eye_pitch + 128), grounded = state.grounded, vertical_motion = signed(velocity.y * 64), hand_contact = state.hand_contact,
      swallowed = state.swallowed },
    retina = state.retina,
    interoception = self.player and felt_body(self.player) or nil,
    capabilities = { body_contract_version = self.player and 6 or 5, checkpoint_resume = false, dead = { chemicals = {not open_world, not open_world, true, true}, vibration = true,
      touch = false, joint_position = false, muscle_tension = false, acceleration = false, orientation = false } },
    transport = transport_name[state.status] or "Disconnected" }
  local body = minetest.write_json({ protocol = protocol, agent_id = agent_id, observation = observation,
    trace = { applied_server_step = state.applied_step, applied_steps = state.applied_steps,
      accumulated_dtime_us = math.floor(state.elapsed * 1000000 + 0.5), applied_action = state.held, pose = pose(self) } })
  -- Hand contact is a bounded event latch: preserve it until the one subsequent observation,
  -- then clear it.  It never carries target identity or any other world fact.
  state.hand_contact = 0
  state.swallowed = 0
  state.pain = 0
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
    state.decisions = state.decisions + 1
    if (state.held.hand_pulse or 0) > 0 then state.pushes = state.pushes + 1 end
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
    local hooks = test_hooks and rawget(_G, "ai_bridge_test")
    if hooks and hooks.override then
      for field, value in pairs(hooks.override(state.sequence) or {}) do state.held[field] = value end
    end
    if learn then learn_step() end
    if mode == "observe" then swing_back(state.observe_cube) end
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
  -- While observing, a push wins over walking instead, so every push is visible.
  local name = "idle"
  local velocity = object:get_velocity()
  local moving = math.sqrt(velocity.x * velocity.x + velocity.z * velocity.z) > 0.08
  local pushing = (action.hand_pulse or 0) > 0
  if pushing and mode == "observe" then
    name = "hand"
  elseif (action.thrust or 0) ~= 0 or (action.strafe or 0) ~= 0 or moving then
    name = "walk"
  elseif pushing then
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

-- An open world unloads blocks far from players, which would remove the unsaved founder. Keep
-- the founder's block and its horizontal neighbours loaded wherever it goes.
local function keep_loaded_around(position)
  local block = vector.floor(vector.divide(position, 16))
  local key = minetest.pos_to_string(block)
  if state.forceloaded_key == key then return end
  for _, old in ipairs(state.forceloaded) do minetest.forceload_free_block(old, true) end
  state.forceloaded, state.forceloaded_key = {}, key
  for dx = -1, 1 do for dz = -1, 1 do
    local node = vector.multiply(vector.add(block, {x = dx, y = 0, z = dz}), 16)
    minetest.forceload_block(node, true)
    state.forceloaded[#state.forceloaded + 1] = node
  end end
end

-- Play mode: the founder is a real VoxeLibre player, named by ai_bridge_founder and connected
-- through its own windowless client. The game gives that player everything a player has
-- (health, hunger, inventory, mobs that hunt it, being saved); the brain drives it from here
-- through the same senses and primitive actions as the entity body. Its hand punches what it
-- reaches or digs the block it looks at, taking the time the block needs; its jaw, held, eats
-- the food in its hand, taking VoxeLibre's eating time.
local founder_name = minetest.settings:get("ai_bridge_founder") or "founder"

-- The founder player, wrapped so the shared sensing and bridge code can treat it like the
-- entity body: position, velocity and facing (yaw).
local function player_body(player)
  local object = {}
  function object:get_pos() return player:get_pos() end
  function object:get_velocity() return player:get_velocity() end
  function object:get_yaw() return player:get_look_horizontal() end
  return {object = object, player = player, eye_height = player:get_properties().eye_height or 1.5}
end

-- What the founder's hand digs and hits with: the held tool, or, as the engine does for any
-- player holding nothing or a non-tool, the bare hand the game gives players (kept in their
-- "hand" inventory list).
local function held(player)
  local stack = player:get_wielded_item()
  if stack:is_empty() or not stack:get_definition().tool_capabilities then
    local hand = player:get_inventory():get_stack("hand", 1)
    if not hand:is_empty() then return hand end
  end
  return stack
end

local function use_hand(player, body)
  local eye = vector.offset(player:get_pos(), 0, body.eye_height, 0)
  local look = player:get_look_dir()
  for pointed in minetest.raycast(eye, vector.add(eye, vector.multiply(look, 4)), true, false) do
    -- The founder's own body and what is attached to it (such as the item shown in its hand) are
    -- not something its hand can reach.
    local own = pointed.type == "object" and (pointed.ref == player or pointed.ref:get_attach() == player)
    if pointed.type == "object" and not own and pointed.ref:get_properties().pointable ~= false then
      local stack = held(player)
      pointed.ref:punch(player, 1.0, stack:get_tool_capabilities(), look)
      state.hand_contact, state.dig = 255, nil
      return
    elseif pointed.type == "node" then
      local node = minetest.get_node(pointed.under)
      local def = minetest.registered_nodes[node.name] or {}
      if def.pointable ~= false then
        state.hand_contact = 255
        local stack = held(player)
        local dig = minetest.get_dig_params(def.groups or {}, stack:get_tool_capabilities(), stack:get_wear())
        if not dig.diggable then state.dig = nil return end
        if state.dig and vector.equals(state.dig.pos, pointed.under) then
          state.dig.progress = state.dig.progress + step_seconds
        else
          state.dig = {pos = pointed.under, progress = step_seconds}
        end
        if state.dig.progress >= dig.time then
          state.dig = nil
          minetest.node_dig(pointed.under, node, player)
        end
        return
      end
    end
  end
  state.dig = nil
end

-- The use pulse: what a player's other hand does on what is within reach, through the game's
-- own handlers: operate it (a door, gate, bed, chest...), or else place or use the held item.
local function use_other_hand(player, body)
  local eye = vector.offset(player:get_pos(), 0, body.eye_height, 0)
  local look = player:get_look_dir()
  local stack = player:get_wielded_item()
  local item = stack:get_definition()
  for pointed in minetest.raycast(eye, vector.add(eye, vector.multiply(look, 4)), true, false) do
    local own = pointed.type == "object" and (pointed.ref == player or pointed.ref:get_attach() == player)
    if pointed.type == "object" and not own then
      local entity = pointed.ref:get_luaentity()
      if entity and entity.on_rightclick then entity:on_rightclick(player) end
      return
    elseif pointed.type == "node" then
      local node = minetest.get_node(pointed.under)
      local def = minetest.registered_nodes[node.name] or {}
      if def.on_rightclick then
        player:set_wielded_item(def.on_rightclick(pointed.under, node, player, stack, pointed) or stack)
      elseif item and item.on_place then
        player:set_wielded_item(item.on_place(stack, player, pointed) or stack)
      end
      return
    end
  end
  if item and item.on_secondary_use then
    player:set_wielded_item(item.on_secondary_use(stack, player, {type = "nothing"}) or stack)
  end
end

-- Brings the next hotbar item into the founder's hand. A server cannot move a player's selected
-- hotbar slot (only its client can), so the hotbar's items move along by one instead: what the
-- hand holds changes exactly as if the next slot were selected.
local function next_item(player)
  local inventory = player:get_inventory()
  local slots = player:hud_get_hotbar_itemcount()
  local hand = player:get_wield_index()
  local hotbar = {}
  for slot = 1, slots do hotbar[slot] = inventory:get_stack("main", slot) end
  for offset = 0, slots - 1 do
    local slot = (hand - 1 + offset) % slots + 1
    inventory:set_stack("main", slot, hotbar[(hand + offset) % slots + 1])
  end
end

local function chew(player, jaw)
  local hunger = rawget(_G, "mcl_hunger")
  local stack = player:get_wielded_item()
  local points = minetest.get_item_group(stack:get_name(), "eatable")
  local full = hunger and hunger.get_hunger(player) >= 20
    and minetest.get_item_group(stack:get_name(), "can_eat_when_full") == 0
  if jaw <= 0 or points == 0 or not hunger or full then
    state.chewing = 0
    return
  end
  state.chewing = (state.chewing or 0) + step_seconds
  if state.chewing < (hunger.EATING_DELAY or 1.61) then return end
  state.chewing = 0
  player:set_wielded_item(hunger.eat(points, nil, stack, player, {type = "nothing"}))
  state.swallowed, state.meals = 255, state.meals + 1
  minetest.log("action", "AI_BRIDGE_MEAL " .. tostring(state.meals))
end

local function grounded(player, velocity)
  local below = minetest.registered_nodes[minetest.get_node(vector.offset(player:get_pos(), 0, -0.1, 0)).name]
  return below ~= nil and below.walkable and math.abs(velocity.y) < 0.5
end

-- One server step of driving the founder player with the held action.
local function drive_founder(player, dtime)
  local body = state.founder_body
  state.server_step, state.applied_steps, state.elapsed = state.server_step + 1, state.applied_steps + 1, state.elapsed + dtime
  if state.pending and (minetest.get_us_time() - state.pending_at) / 1000000 > request_timeout then
    state.request_token = state.request_token + 1 -- late reply cannot replace the safety action
    state.pending = false
    zero_action("timeout")
  end
  if player:get_hp() <= 0 then return end
  local velocity = player:get_velocity()
  state.grounded = grounded(player, velocity) and 255 or 0
  local action, yaw = state.held, player:get_look_horizontal()
  local v2_angle = yaw + math.pi / 2
  local forward = {x = math.cos(v2_angle), z = math.sin(v2_angle)}
  local right = {x = -forward.z, z = forward.x}
  local thrust = clamp((action.thrust or 0) * 4 / 256, -4, 4)
  local strafe = clamp((action.strafe or 0) * 4 / 256, -4, 4)
  local moving = thrust ~= 0 or strafe ~= 0
  -- Touch: pushing to move but held in place by something.
  state.touch = (moving and math.abs(velocity.x) + math.abs(velocity.z) < 0.3) and 255 or 0
  if moving then
    player:add_velocity({x = forward.x * thrust + right.x * strafe - velocity.x, y = 0,
      z = forward.z * thrust + right.z * strafe - velocity.z})
  end
  if state.applied_steps == 1 then
    local turn = clamp(trunc_toward_zero((action.turn or 0) / 32), -8, 8)
    player:set_look_horizontal(yaw + turn * (2 * math.pi / 256))
    state.eye_pitch = clamp(state.eye_pitch + trunc_toward_zero((action.look_pitch or 0) / 32), -128, 127)
    player:set_look_vertical(-state.eye_pitch * math.pi / 512)
    if (action.jump or 0) > 0 and state.grounded > 0 then
      player:add_velocity({x = 0, y = 6.5, z = 0})
      local hunger = rawget(_G, "mcl_hunger")
      if hunger and hunger.exhaust then hunger.exhaust(founder_name, hunger.EXHAUST_JUMP or 50) end
    end
    if (action.hand_pulse or 0) > 0 then use_hand(player, body) else state.dig = nil end
    if (action.use_pulse or 0) > 0 then use_other_hand(player, body) end
    if (action.next_slot or 0) > 0 then next_item(player) end
    chew(player, action.jaw or 0)
  end
  if not state.pending and state.elapsed >= step_seconds then send_transition(body, state.elapsed) end
end

if play then
  minetest.register_globalstep(function(dtime)
    local player = state.founder
    if player and player:get_pos() and state.founder_body then drive_founder(player, dtime) end
  end)
  minetest.register_on_player_hpchange(function(player, change)
    if change < 0 and player:get_player_name() == founder_name then state.pain = (state.pain or 0) - change end
  end)
  minetest.register_on_dieplayer(function(player)
    if player:get_player_name() ~= founder_name then return end
    state.deaths = (state.deaths or 0) + 1
    -- Its client has no one to press Respawn: the founder is born again after a moment.
    minetest.after(3, function()
      if player:get_pos() and player:get_hp() <= 0 then player:respawn() end
    end)
  end)
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
    if open_world then keep_loaded_around(position) end
    local escaped = open_world and position.y < founder_spawn.y - open_world_fall_limit
      or not open_world and (math.abs(position.x) > arena_safety_limit.horizontal
        or math.abs(position.z) > arena_safety_limit.horizontal
        or position.y < arena_safety_limit.low_y or position.y > arena_safety_limit.high_y)
    if escaped then
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
      if (action.jaw or 0) > 0 then eat_in_front(self) end
      if (action.hand_pulse or 0) > 0 then
        local origin = vector.add(self.object:get_pos(), {x = forward.x * 0.8, y = 0.8, z = forward.z * 0.8})
        -- Deliberately bounded reach around the visible forward hand; this is contact feedback,
        -- not a proximity sensor or a way to identify arbitrary world objects.
        for _, object in ipairs(minetest.get_objects_inside_radius(origin, 1.2)) do
          local entity = object:get_luaentity()
          if entity and entity.name == "ai_bridge:test_object" and not learn_inert then
            object:add_velocity({x = forward.x * 3, y = 1, z = forward.z * 3}); state.hand_contact = 255
            state.pushed_at, state.hits = state.decisions, state.hits + 1
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

-- Open world: find land near the origin from the map generator's own spawn rule, generate the
-- surroundings, and set the founder on the surface there. No arena and no test cube.
-- Edible blocks that grow on the ground, where the founder's mouth can reach them (fruit high
-- in a canopy cannot be).
local function ground_food(minp, maxp)
  local names = {}
  for name in pairs(minetest.registered_nodes) do
    if edible(name) then names[#names + 1] = name end
  end
  local reachable = {}
  for _, position in ipairs(minetest.find_nodes_in_area(minp, maxp, names)) do
    local below = minetest.registered_nodes[minetest.get_node(vector.offset(position, 0, -1, 0)).name]
    if below and below.walkable and (below.groups or {}).leaves == nil then
      reachable[#reachable + 1] = position
    end
  end
  return reachable
end

-- Where the founder is born: beside the densest patch of ground food in a wide stretch of the
-- generated world, as an animal is born in its habitat. It must still find, approach and eat the
-- food itself; nothing about the food is told to it.
local open_world_search = 96

-- Feet position on the first solid, non-leaf ground below `top` with two free nodes above it,
-- or nil over water, in a canopy, or with no room to stand.
local function standing_place(x, top, z, depth)
  local function def_at(y) return minetest.registered_nodes[minetest.get_node({x = x, y = y, z = z}).name] end
  for y = top, top - (depth or 24), -1 do
    local ground = def_at(y)
    if ground and ground.walkable then
      local head, above = def_at(y + 1), def_at(y + 2)
      if (ground.groups or {}).leaves or (ground.groups or {}).tree
        or not head or head.walkable or not above or above.walkable then
        return nil
      end
      return {x = x, y = y + 0.5, z = z}
    end
    if ground and ground.liquidtype ~= "none" then return nil end
  end
  return nil
end

-- Whether the ground two nodes around `feet` is within a node of its height: no cliff edge, ravine
-- or pond next to a newborn.
local function level_around(feet)
  for dx = -2, 2 do
    for dz = -2, 2 do
      local ground = standing_place(feet.x + dx, math.floor(feet.y) + 3, feet.z + dz, 5)
      if not ground or math.abs(ground.y - feet.y) > 1 then return false end
    end
  end
  return true
end

-- Calls `found` with a place on open, dry ground under the sky within 32 nodes of `centre`
-- (never in a cave, water or lava), or with nil if there is none.
local function surface_near(centre, found)
  local x, z = math.floor(centre.x + 0.5), math.floor(centre.z + 0.5)
  local top = math.floor(centre.y) + 64
  local minp = {x = x - 32, y = math.floor(centre.y) - 48, z = z - 32}
  minetest.emerge_area(minp, {x = x + 32, y = top, z = z + 32}, function(_, _, remaining)
    if remaining ~= 0 then return end
    for radius = 3, 32 do
      for _, offset in ipairs({{radius, 0}, {-radius, 0}, {0, radius}, {0, -radius},
          {radius, radius}, {-radius, -radius}, {radius, -radius}, {-radius, radius}}) do
        local feet = standing_place(x + offset[1], top, z + offset[2], top - minp.y)
        if feet and level_around(feet) then found(feet) return end
      end
    end
    found(nil)
  end)
end

local function birthplace(food)
  local best, best_count = nil, 0
  for _, candidate in ipairs(food) do
    local count = 0
    for _, other in ipairs(food) do
      if vector.distance(candidate, other) <= 12 then count = count + 1 end
    end
    if count > best_count then best, best_count = candidate, count end
  end
  if not best then return nil end
  -- Stand a few nodes from the patch, on the first open ground found around it.
  for radius = 3, 12 do
    for _, offset in ipairs({{radius, 0}, {-radius, 0}, {0, radius}, {0, -radius}}) do
      local feet = standing_place(best.x + offset[1], best.y + 8, best.z + offset[2])
      if feet then return feet end
    end
  end
  return nil
end

local function place_founder(spawn)
  founder_spawn = spawn
  minetest.log("action", "AI_BRIDGE_OPEN_WORLD spawn " .. minetest.pos_to_string(founder_spawn))
  keep_loaded_around(founder_spawn)
  local corner = vector.new(32, 32, 32)
  minetest.emerge_area(vector.subtract(founder_spawn, corner), vector.add(founder_spawn, corner),
    function(_, _, remaining)
      if remaining ~= 0 then return end
      local near = ground_food(vector.subtract(founder_spawn, corner), vector.add(founder_spawn, corner))
      local nearest, kinds = math.huge, {}
      for _, position in ipairs(near) do
        nearest = math.min(nearest, vector.distance(founder_spawn, position))
        local name = minetest.get_node(position).name
        kinds[name] = (kinds[name] or 0) + 1
      end
      local listed = {}
      for name, count in pairs(kinds) do listed[#listed + 1] = name .. "=" .. count end
      table.sort(listed)
      -- Operator log only; the founder never receives it.
      minetest.log("action", ("AI_BRIDGE_FOOD reachable_within_32=%d nearest=%s %s"):format(#near,
        nearest == math.huge and "none" or ("%.1f"):format(nearest), table.concat(listed, " ")))
      minetest.add_entity(founder_spawn, "ai_bridge:founder")
    end)
end

local function settle_in_open_world()
  minetest.set_timeofday(0.5)
  local minp = {x = -open_world_search, y = -16, z = -open_world_search}
  local maxp = {x = open_world_search, y = 64, z = open_world_search}
  minetest.emerge_area(minp, maxp, function(_, _, remaining)
    if remaining ~= 0 then return end
    local spawn = birthplace(ground_food(minp, maxp))
    if not spawn then
      local level = minetest.get_spawn_level(0, 0) or 20
      spawn = {x = 0, y = level - 0.5, z = 0}
    end
    place_founder(spawn)
  end)
end

minetest.register_on_mods_loaded(function()
  minetest.after(0, function()
    if play then
      -- VoxeLibre leaves the world spawn unset until something sets it, and Luanti's fallback
      -- can land a person in a cave or lava. Make the world spawn open, level surface ground.
      local spawn_api = rawget(_G, "mcl_spawn")
      local current = spawn_api and spawn_api.get_world_spawn_pos()
      if current and current.x == 0 and current.y == 8 and current.z == 0 then
        surface_near({x = 0, y = minetest.get_spawn_level(0, 0) or 20, z = 0}, function(feet)
          if feet then
            spawn_api.set_world_spawn_pos(feet, false)
            minetest.log("action", "AI_BRIDGE_WORLD_SPAWN " .. minetest.pos_to_string(feet))
          end
        end)
      end
      return
    end
    if open_world then
      settle_in_open_world()
      return
    end
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
      elseif mode == "observe" then
        state.observe_cube = minetest.add_entity(learn_cube_spawn, "ai_bridge:test_object")
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
  if play then
    -- In a shared game the founder is a player.
    if state.founder and state.founder:get_pos() then list[1] = {object = state.founder, label = "Founder 1"} end
    return list
  end
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

-- An observer of a founder player rides an invisible camera that follows the founder's eyes:
-- VoxeLibre expects anything a player is attached to to be an entity (like a boat or horse).
minetest.register_entity("ai_bridge:camera", {
  initial_properties = {physical = false, pointable = false, is_visible = false,
    collisionbox = {0, 0, 0, 0, 0, 0}, static_save = false},
})

local function leave_pov(player)
  if pov.target and pov.target:get_pos() and pov.target:is_player() then
    -- The founder's body is seen by everyone again.
    pov.target:set_observers(nil)
  end
  pov.target = nil
  if pov.camera then
    if player then player:set_detach() end
    pov.camera:remove()
    pov.camera = nil
  end
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
  if founder:is_player() then
    -- A founder player's eyes are at a player's eye height already, so the camera sits exactly
    -- there. Like one's own head, the founder's body is not seen from inside it: hidden from
    -- everyone but the founder while observed (a player must always observe itself).
    founder:set_observers({[founder:get_player_name()] = true})
    pov.camera = minetest.add_entity(founder:get_pos(), "ai_bridge:camera")
    pov.camera:set_yaw(founder:get_look_horizontal())
    player:set_attach(pov.camera, "", {x = 0, y = 0, z = 0}, {x = 0, y = 0, z = 0})
    return
  end
  player:set_attach(founder, "", {x = 0, y = -6, z = 4}, {x = 0, y = 0, z = 0})
end

-- 16 rays cover 360 degrees; increasing ray index turns left. Shown left to right with the ray
-- straight ahead in the middle and the ray straight behind at the far left.
local EYE_STRIP_ORDER = {8, 7, 6, 5, 4, 3, 2, 1, 0, 15, 14, 13, 12, 11, 10, 9}

local function update_eye_strip(player)
  local retinal = state.last_retinal
  if not retinal then return end
  hud_set(player, "eye_panel", {type = "image", position = {x = 0.5, y = 0},
    offset = {x = 0, y = 16}, alignment = {x = 0, y = 1},
    text = "ai_bridge_base.png^[colorize:#101418:255^[opacity:185",
    scale = {x = 46, y = 7}, z_index = 0})
  hud_set(player, "eye_label", {type = "text", position = {x = 0.5, y = 0},
    offset = {x = 0, y = 26}, alignment = {x = 0, y = 1}, number = 0xFFE08A, z_index = 1,
    text = "WHAT THE FOUNDER SENSES  |  16 eye rays all around, ahead in the middle"})
  for slot, ray in ipairs(EYE_STRIP_ORDER) do
    local sample = retinal[ray + 1]
    local colour = string.format("%02x%02x%02x", sample.red, sample.green, sample.blue)
    local texture = "ai_bridge_base.png^[colorize:#" .. colour .. ":255"
    hud_set(player, "eye_ray" .. slot, {type = "image", position = {x = 0.5, y = 0},
      offset = {x = (slot - 8.5) * 44, y = 56}, scale = {x = 2.5, y = 2.5},
      alignment = {x = 0, y = 1}, text = texture, z_index = 1}, "text", texture)
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
  lines[#lines + 1] = {play and "Type /founder to return to yourself"
    or pov.target and "Press I, then Free camera to leave" or "Press I to watch a founder", 0xB8C4CE}
  -- Keep a fixed number of rows so a founder leaving clears its line.
  local rows = math.max(#lines, pov.rows or 0)
  pov.rows = rows
  hud_panel(player, "right_panel", "right", #lines)
  for row = 1, rows do
    local line = lines[row] or {"", 0xFFFFFF}
    hud_text(player, "right_" .. row, "right", row - 1, line[1], line[2])
  end
end

-- The founder's own hearts, hunger and air, drawn like a player's with VoxeLibre's icons, above
-- the eye strip while observing it in play mode.
-- The founder player's hunger as the game keeps it.
local function founder_hunger(founder)
  local hunger = rawget(_G, "mcl_hunger")
  return hunger and hunger.get_hunger(founder) or 20
end

local function update_life_bars(player, founder)
  local bars = {
    {key = "founder_hp", icon = "hudbars_icon_health.png", bg = "hudbars_bgicon_health.png",
      value = founder:get_hp(), max = 20, x = -265},
    {key = "founder_food", icon = "hbhunger_icon.png", bg = "hbhunger_bgicon.png",
      value = founder_hunger(founder), max = 20, x = 25},
  }
  local air, air_max = founder:get_breath(), founder:get_properties().breath_max or 10
  local breath = air < air_max
  bars[3] = {key = "founder_air", icon = "hudbars_icon_breath.png", bg = "hudbars_bgicon_breath.png",
    value = breath and math.floor(air * 20 / air_max) or 0, max = breath and 20 or 0, x = 25, y = -170}
  for _, bar in ipairs(bars) do
    hud_set(player, bar.key, {type = "statbar", position = {x = 0.5, y = 1}, text = bar.icon, text2 = bar.bg,
      number = bar.value, item = bar.max, direction = 0, size = {x = 24, y = 24},
      offset = {x = bar.x, y = bar.y or -140}, z_index = 1}, "number", bar.value)
    player:hud_change(hud[bar.key], "item", bar.max)
  end
end

local function update_pov(player)
  update_founder_list(player)
  local founder = pov.target
  if not founder then return end
  if not founder:get_pos() then
    -- In play mode the observer keeps watching through the death and follows the reborn founder.
    if play then
      if state.founder and state.founder:get_pos() then enter_pov(player, state.founder) end
    else
      leave_pov(player)
    end
    return
  end
  if play then update_life_bars(player, founder) end
  if pov.camera then
    if not pov.camera:get_pos() then enter_pov(player, founder) return end
    pov.camera:move_to(founder:get_pos(), true)
    pov.camera:set_yaw(founder:get_look_horizontal())
  end
  -- Look exactly where the founder's eyes point: its yaw, and its eye pitch (positive is up).
  player:set_look_horizontal(founder:is_player() and founder:get_look_horizontal() or founder:get_yaw())
  player:set_look_vertical(-state.eye_pitch * math.pi / 512)
  update_eye_strip(player)
end

minetest.register_on_player_receive_fields(function(player, formname, fields)
  if play or player:get_player_name() ~= observer_name then return end
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

-- Play mode: what the person had before observing, restored when they return to themselves.
local saved_self

local function clear_hud(player)
  for _, id in pairs(hud) do player:hud_remove(id) end
  hud, pov.rows = {}, 0
end

-- The person's own hotbar and status bars (health, hunger, armour...) belong to their body, not
-- the founder's, so they are hidden while observing. VoxeLibre draws its bars with vl_hudbars.
local function own_bars(name)
  local bars = rawget(_G, "vl_hudbars")
  return bars, bars and bars.players and bars.players[name] and bars.players[name].hudbar_states or {}
end

-- VoxeLibre shows a person's bars again whenever it updates them; while they observe, those
-- requests are held back so their own bars stay hidden (without flickering).
local function hold_back_own_bars(bars)
  if not bars or bars.ai_bridge_held_back then return end
  bars.ai_bridge_held_back = true
  local show = bars.show
  bars.show_own = show
  bars.show = function(player, ...)
    if saved_self and player and player.get_player_name and player:get_player_name() == observer_name then return end
    return show(player, ...)
  end
end

local function hide_own_hud(player)
  local name = player:get_player_name()
  local bars, states = own_bars(name)
  hold_back_own_bars(bars)
  local shown = {}
  for identifier, bar in pairs(states) do
    local visible = bar.state and not bar.state.hidden
    for _, part in pairs(bar.parts or {}) do
      visible = visible or (part.state and not part.state.hidden)
    end
    if visible then
      shown[#shown + 1] = identifier
      bars.hide(player, identifier)
    end
  end
  local flags = player:hud_get_flags()
  player:hud_set_flags({hotbar = false, healthbar = false, breathbar = false, wielditem = false})
  return {bars = shown, flags = flags}
end

local function show_own_hud(player, saved)
  player:hud_set_flags(saved.flags)
  local bars = own_bars(player:get_player_name())
  for _, identifier in ipairs(saved.bars) do (bars.show_own or bars.show)(player, identifier) end
end

local function become_observer(player)
  local name = player:get_player_name()
  local privs = minetest.get_player_privs(name)
  saved_self = {pos = player:get_pos(), privs = table.copy(privs), armor = player:get_armor_groups()}
  saved_self.hud = hide_own_hud(player)
  state.observing = true
  -- Kept in the person's own data too, so a crash while observing cannot cost them privileges.
  player:get_meta():set_string("ai_bridge:privs", minetest.privs_to_string(saved_self.privs))
  -- An observer only watches: no building or breaking, no damage, and no visible body.
  privs.interact = nil
  minetest.set_player_privs(name, privs)
  player:set_armor_groups({immortal = 1})
  -- The observer's hidden body rides at the founder's eyes; mobs must not take it for a target
  -- (VoxeLibre's monsters attack the nearest player they can see, and ignore invisible ones).
  local mobs = rawget(_G, "mcl_mobs")
  if mobs and mobs.invis then mobs.invis[name] = true end
  -- Invisible rather than shrunk: VoxeLibre divides by the body's size (to fit flames to it).
  player:set_properties({is_visible = false})
  enter_pov(player, state.founder)
end

local function become_self(player)
  if not saved_self then return end
  local back = saved_self
  saved_self = nil
  state.observing = false
  leave_pov(player)
  clear_hud(player)
  minetest.set_player_privs(player:get_player_name(), back.privs)
  player:get_meta():set_string("ai_bridge:privs", "")
  player:set_armor_groups(back.armor)
  player:set_properties({is_visible = true})
  local mobs = rawget(_G, "mcl_mobs")
  if mobs and mobs.invis then mobs.invis[player:get_player_name()] = nil end
  local burning = rawget(_G, "mcl_burning")
  if burning and burning.extinguish then burning.extinguish(player) end
  show_own_hud(player, back.hud)
  player:set_pos(back.pos)
  -- The detach completes on the next server step; place the body again once it has.
  minetest.after(0.2, function()
    if player:get_pos() then player:set_pos(back.pos) end
  end)
end

-- An observer's body travels with the founder, through whatever lava, fire or rock he meets;
-- it takes no harm while observing.
minetest.register_on_player_hpchange(function(player, change)
  if saved_self and change < 0 and player:get_player_name() == observer_name then return 0 end
  return change
end, true)

minetest.register_chatcommand("founder", {
  description = "Switch between playing as yourself and observing through the founder's eyes; "
    .. "'/founder here' brings him to open ground near you.",
  params = "[here]",
  func = function(name, param)
    if not play then return false, "/founder is for play mode; observers use /founders." end
    if name ~= observer_name then return false, "Only " .. observer_name .. " may observe the founder." end
    local player = minetest.get_player_by_name(name)
    if not player then return false end
    if param == "here" then
      if saved_self then return false, "Return to yourself first: /founder" end
      surface_near(player:get_pos(), function(feet)
        if not feet then
          minetest.chat_send_player(name, "No open ground near you for the founder.")
          return
        end
        -- His birthplace moves too, so he is born again here if he dies.
        founder_spawn = feet
        if not (state.founder and state.founder:get_pos()) then
          minetest.chat_send_player(name, "The founder is not connected.")
          return
        end
        state.founder:set_pos(feet)
        minetest.chat_send_player(name, "The founder is beside you, at " .. minetest.pos_to_string(feet) .. ".")
      end)
      return true, "Bringing the founder to open ground near you..."
    end
    if saved_self then
      become_self(player)
      return true, "You are yourself again."
    end
    if not state.founder or not state.founder:get_pos() then
      return false, "The founder is not connected yet."
    end
    become_observer(player)
    return true, "Observing through the founder's eyes. Type /founder to return to yourself."
  end,
})

minetest.register_on_leaveplayer(function(player)
  if play and player:get_player_name() == founder_name then
    state.founder, state.founder_body = nil, nil
    zero_action("disconnected")
    return
  end
  if play then
    -- Leave as oneself, so privileges and position are kept for the next session.
    if saved_self then become_self(player) end
    hud, pov.rows = {}, 0
    return
  end
  if player:get_player_name() == observer_name then
    leave_pov(nil)
    hud, pov.rows = {}, 0
  end
end)

-- Whether a person standing here is safe: solid ground below, room to breathe, no lava or fire.
local function safe_to_stand(position)
  local function def_at(dy)
    return minetest.registered_nodes[minetest.get_node(vector.offset(position, 0, dy, 0)).name] or {}
  end
  local below, feet, head = def_at(-0.6), def_at(0.2), def_at(1.5)
  for _, def in ipairs({feet, head}) do
    if def.walkable or (def.liquidtype or "none") ~= "none" or (def.damage_per_second or 0) > 0 then
      return false
    end
  end
  return below.walkable == true
end

-- Moves a person in danger (in lava, water or a wall) to the world spawn's open ground.
local function rescue(player)
  local position = player:get_pos()
  if not position or player:get_hp() <= 0 or safe_to_stand(position) then return end
  local spawn_api = rawget(_G, "mcl_spawn")
  local spawn = spawn_api and spawn_api.get_world_spawn_pos() or {x = 0, y = 20, z = 0}
  surface_near(spawn, function(feet)
    if feet and player:get_pos() then
      player:set_pos(feet)
      minetest.chat_send_player(player:get_player_name(), "You arrived somewhere dangerous; moved to open ground.")
    end
  end)
end

if play then
  minetest.register_on_respawnplayer(function(player)
    minetest.after(0.5, function() if player:get_pos() then rescue(player) end end)
  end)
end

minetest.register_on_joinplayer(function(player)
  if play then
    if player:get_player_name() == founder_name then
      state.founder, state.founder_body = player, player_body(player)
      state.eye_pitch = 0
      minetest.log("action", "AI_BRIDGE_FOUNDER_PLAYER joined")
      -- The first time ever, the founder is born on open ground near the person playing.
      local meta = player:get_meta()
      if meta:get_string("ai_bridge:born") == "" then
        meta:set_string("ai_bridge:born", "yes")
        minetest.after(1, function()
          local person = minetest.get_player_by_name(observer_name)
          local spawn_api = rawget(_G, "mcl_spawn")
          local near = person and person:get_pos() or spawn_api and spawn_api.get_world_spawn_pos()
            or {x = 0, y = 20, z = 0}
          surface_near(near, function(feet)
            if feet and player:get_pos() then player:set_pos(feet) end
          end)
        end)
      else
        minetest.after(0.5, function() if player:get_pos() then rescue(player) end end)
      end
      return
    end
    hud, pov.rows = {}, 0
    -- A person who was observing when the server stopped gets their own privileges back.
    local kept = player:get_meta():get_string("ai_bridge:privs")
    if kept ~= "" then
      minetest.set_player_privs(player:get_player_name(), minetest.string_to_privs(kept))
      player:get_meta():set_string("ai_bridge:privs", "")
    end
    minetest.after(0.5, function() if player:get_pos() then rescue(player) end end)
    return
  end
  if player:get_player_name() ~= observer_name then return end
  hud, pov.rows = {}, 0
  player:set_inventory_formspec(founder_picker())
  -- The observer is a normal, non-privileged explorer. Its controls affect only the player:
  -- the founder remains a separate entity whose actuators are driven only by Rust bridge replies.
  -- Aim at the founder's chest from far enough away to show the complete humanoid and the one
  -- nearby neutral fixture without making either fill the first frame.
  minetest.set_player_privs(observer_name, {})
  player:set_physics_override({speed = 1, jump = 1, gravity = 1})
  player:set_pos(open_world and vector.add(founder_spawn, {x = 0, y = 1.6, z = -4})
    or {x = 0, y = 2.1, z = -3})
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
  if play then
    -- Only the person observing sees the founder's HUD.
    local watcher = saved_self and pov.target and minetest.get_player_by_name(observer_name)
    if watcher then
      update_observer_hud()
      update_pov(watcher)
    end
    return
  end
  update_observer_hud()
  local player = minetest.get_player_by_name(observer_name)
  if player then update_pov(player) end
end)
