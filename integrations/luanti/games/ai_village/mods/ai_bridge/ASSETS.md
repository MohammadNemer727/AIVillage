# AI Bridge assets

## Founder player media: Minetest Game / player_api

The founder uses the actual Minetest Game player media, copied byte-for-byte from the
installed Fedora package so the `ai_village` game remains self-contained at runtime:

- `models/minetest_game_character.b3d` comes from
  `/var/lib/minetest/.minetest/games/minetest_game/mods/player_api/models/character.b3d`
  (`69a9b760487f0938929aaf3e0ef49c68bb8feb1e213f78544d2c23a71e83c98b`).
- `textures/minetest_game_character.png` comes from
  `/var/lib/minetest/.minetest/games/minetest_game/mods/player_api/models/character.png`
  (`351626fcb8155d6285315a213ff4ae668a9607a3eda776663512dd799ec8437a`).

Source package: Fedora `minetest-server-5.17.0-1.fc44.x86_64`, source RPM
`minetest-5.17.0-1.fc44.src.rpm`; installed Luanti version: `5.17.0`.
The authoritative provenance is the installed
`mods/player_api/{README.txt,license.txt}`: `character.b3d` was originally made by
MirceaKitsune and altered/fixed by kilbith, sofar, xunto, Rogier-5, TeTpaAka, Desour,
stujones11, and An0n3m0us; `character.png` is by Jordach.

Both files are CC BY-SA 3.0. This permits copying, redistribution, modification, and
commercial use provided attribution, the license link, and ShareAlike are retained. This
project copies the files unchanged and records this attribution; downstream redistributors
must preserve it and license adaptations under [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/).
No installed Minetest Game assets are required at runtime because these two media files are
vendored. Updating them requires an explicit provenance/license review and hash update.

`character.b3d` defines native player frames `0–79` (idle), `168–187` (walk), and
`189–198` (mine/hand action) at 30 fps. It contains no jump animation; the founder keeps
the native idle pose during a jump rather than inventing or borrowing a non-player frame.
`init.lua` selects only these native frame ranges from the current Rust body action. Because
the mesh supports one animation at once, locomotion has explicit priority: any nonzero
thrust/strafe command (or residual horizontal physical motion) uses `168–187`; the hand
range is used only for an isolated nonzero hand pulse.

## Arena containment

The visible room has a solid inner floor, walls, and ceiling sized for the founder's
`0.6 × 1.7 × 0.6` collisionbox. Luanti node centres are integral, so the founder's
collisionbox bottom is spawned at `y=0.5`, on the floor node's upper face. A second solid
outer floor/wall/roof shell prevents a high-speed jump or strafe from finding an edge around
that inner collision surface.
If a Luanti physics failure nevertheless places the founder outside that shell, Lua performs
an explicit `AI_BRIDGE_SAFETY_RESET`, logs it, and displays its count/reason in the observer
HUD. This is physical scene recovery only: it does not alter the held Rust action, protocol
sequence, intrinsic choice, or learning state.

The legacy original `models/ai_bridge_founder_cc0.obj` and its `*_cc0.png` textures remain
in the mod but are not registered by the founder. They are CC0-1.0 AI Village artwork.

## Generic interaction fixture
The sole nearby grey cube is an original generic physics fixture using
`ai_bridge_generic_cc0.png` (CC0-1.0). Its `0.4` visual scale matches its
`0.4 × 0.4 × 0.4` collisionbox; it starts at `(1.2, 0.7, 0)` beside the founder and has no
semantic name, material identity, or label in the Rust protocol.

`palette_generated.lua` and `textures/palette.json` are generated data for the original
test-room palette; see the generator for source hashes and regeneration checks.
